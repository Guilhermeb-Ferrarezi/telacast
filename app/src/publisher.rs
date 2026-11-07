use openh264::{
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod},
    formats::{BgraSliceU8, YUVBuffer},
    OpenH264API,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::{connect, Message};
use x11rb::{
    connection::Connection,
    protocol::{randr::ConnectionExt as _, xproto::{ConnectionExt as _, ImageFormat}},
};

pub struct Handle {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
}

struct Mon {
    name: String,
    x: i16,
    y: i16,
    w: u16,
    h: u16,
}

fn list_monitors() -> anyhow::Result<Vec<Mon>> {
    let (conn, num) = x11rb::connect(None)?;
    let root = conn.setup().roots[num].root;
    let mons = conn.randr_get_monitors(root, true)?.reply()?;
    let mut out = Vec::new();
    for m in mons.monitors {
        let name = conn
            .get_atom_name(m.name)?
            .reply()
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default();
        out.push(Mon { name, x: m.x, y: m.y, w: m.width, h: m.height });
    }
    Ok(out)
}

pub fn monitor_names() -> Vec<String> {
    list_monitors()
        .map(|v| {
            v.iter()
                .map(|m| format!("{} ({}x{})", m.name, m.w, m.h))
                .collect()
        })
        .unwrap_or_default()
}

pub fn start(url: String, monitor: usize, fps: u32, kbps: u32) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("conectando…".to_string()));
    let (s, st) = (stop.clone(), status.clone());
    std::thread::spawn(move || {
        let msg = match run(&url, monitor, fps, kbps, &s, &st) {
            Ok(()) => "parado".to_string(),
            Err(e) => format!("erro: {e:#}"),
        };
        *st.lock().unwrap() = msg;
    });
    Handle { stop, status }
}

fn run(
    url: &str,
    monitor: usize,
    fps: u32,
    kbps: u32,
    stop: &AtomicBool,
    status: &Mutex<String>,
) -> anyhow::Result<()> {
    let mon = list_monitors()?
        .into_iter()
        .nth(monitor)
        .ok_or_else(|| anyhow::anyhow!("tela não encontrada"))?;
    let (conn, num) = x11rb::connect(None)?;
    let root = conn.setup().roots[num].root;
    // H.264 exige dimensões pares.
    let (w, h) = ((mon.w & !1) as usize, (mon.h & !1) as usize);
    let (mut ws, _) = connect(url)?;

    // Captura e codificação em threads separadas: o fps é limitado pelo
    // mais lento dos dois, não pela soma.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
    let cap_stop = Arc::new(AtomicBool::new(false));
    let cs = cap_stop.clone();
    let capture = std::thread::spawn(move || -> anyhow::Result<()> {
        let period = Duration::from_secs_f64(1.0 / fps as f64);
        while !cs.load(Ordering::Relaxed) {
            let tick = Instant::now();
            // ZPixmap de 32 bits por pixel: B, G, R, X.
            let img = conn
                .get_image(ImageFormat::Z_PIXMAP, root, mon.x, mon.y, w as u16, h as u16, !0)?
                .reply()?;
            if img.data.len() < w * h * 4 {
                anyhow::bail!("formato de tela não suportado (precisa de 32 bpp)");
            }
            // Se o encoder ainda está ocupado, descarta o quadro.
            let _ = tx.try_send(img.data);
            if let Some(rest) = period.checked_sub(tick.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        Ok(())
    });

    let (mut sent, mut t0) = (0u32, Instant::now());
    let mut recv = || -> Option<Vec<u8>> {
        while !stop.load(Ordering::Relaxed) {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(d) => return Some(d),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return None, // captura terminou (erro)
            }
        }
        None
    };
    let mut send = |key: bool, h264: &[u8]| -> anyhow::Result<()> {
        let mut payload = Vec::with_capacity(h264.len() + 1);
        payload.push(key as u8);
        payload.extend_from_slice(h264);
        ws.send(Message::Binary(payload.into()))?;
        sent += 1;
        if t0.elapsed() >= Duration::from_secs(1) {
            *status.lock().unwrap() = format!("transmitindo · {sent} fps");
            (sent, t0) = (0, Instant::now());
        }
        Ok(())
    };
    *status.lock().unwrap() = "iniciando encoder…".into();
    let result = match crate::gpu::encode_loop(w as u32, h as u32, fps, kbps, &mut recv, &mut send) {
        Err(e) if e.downcast_ref::<crate::gpu::InitError>().is_some() => {
            // Sem NVENC: cai para o encoder de software.
            *status.lock().unwrap() = format!("{e} — usando CPU");
        let cfg = EncoderConfig::new()
            .bitrate(BitRate::from_bps(kbps * 1000))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2))
            .num_threads(4);
        let mut enc = Encoder::with_api_config(OpenH264API::from_source(), cfg)?;
            (|| -> anyhow::Result<()> {
                while let Some(data) = recv() {
                    let yuv = YUVBuffer::from_bgra8_source(BgraSliceU8::new(&data, (w, h)));
                    let bits = enc.encode(&yuv)?;
                    let key = matches!(
                        bits.frame_type(),
                        openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
                    );
                    let mut buf = Vec::new();
                    bits.write_vec(&mut buf);
                    if !buf.is_empty() {
                        send(key, &buf)?;
                    }
                }
                Ok(())
            })()
        }
        other => other,
    };
    cap_stop.store(true, Ordering::Relaxed);
    let cap_res = capture.join().map_err(|_| anyhow::anyhow!("thread de captura caiu"))?;
    result?;
    cap_res?;
    let _ = ws.close(None);
    Ok(())
}

/// `telacast --bench`: mede captura e codificação sem rede nem interface.
pub fn bench(fps: u32, kbps: u32) -> anyhow::Result<()> {
    let mon = list_monitors()?.into_iter().next().ok_or_else(|| anyhow::anyhow!("sem tela"))?;
    let (conn, num) = x11rb::connect(None)?;
    let root = conn.setup().roots[num].root;
    let (w, h) = ((mon.w & !1) as u16, (mon.h & !1) as u16);
    let grab = || -> anyhow::Result<Vec<u8>> {
        Ok(conn.get_image(ImageFormat::Z_PIXMAP, root, mon.x, mon.y, w, h, !0)?.reply()?.data)
    };
    let t = Instant::now();
    let mut frame = grab()?;
    for _ in 0..59 {
        frame = grab()?;
    }
    println!("captura {w}x{h}: {:.1} fps", 60.0 / t.elapsed().as_secs_f64());

    let (mut n, mut bytes, mut keys) = (0u32, 0usize, 0u32);
    let t = Instant::now();
    crate::gpu::encode_loop(
        w as u32,
        h as u32,
        fps,
        kbps,
        || {
            n += 1;
            (n <= 300).then(|| frame.clone())
        },
        |key, d| {
            bytes += d.len();
            keys += key as u32;
            Ok(())
        },
    )?;
    let s = t.elapsed().as_secs_f64();
    println!("NVENC: {:.1} fps ({} quadros, {} chaves, {:.1} Mbps)", 300.0 / s, 300, keys, bytes as f64 * 8.0 / 1e6 / (300.0 / fps as f64));
    Ok(())
}

use crate::capture::{Grabber, Raw, Scaler, Source};
use openh264::{
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType},
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
use tungstenite::Message;

/// Quadro reduzido (RGBA) do que está sendo transmitido, para a prévia da própria tela (~10 por segundo).
pub type Preview = Arc<Mutex<Option<(u32, u32, Vec<u8>)>>>;

pub struct Handle {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub preview: Preview,
    /// Largura (em pixels físicos) em que a prévia é mostrada; a captura reduz exatamente a isso (sem 2º reescalonamento).
    pub preview_width: Arc<std::sync::atomic::AtomicU32>,
}

/// Reduz um quadro BGRA `w`x`h` para `target` pixels de largura (no máximo, nunca amplia) com média de área
/// e vira RGBA. Com `target` igual ao tamanho em que a prévia aparece, a imagem sai 1:1, sem reescalonar de novo.
fn make_preview(frame: &[u8], w: usize, h: usize, target: usize, scaler: &mut Scaler) -> Option<(u32, u32, Vec<u8>)> {
    if w == 0 || h == 0 || frame.len() < w * h * 4 {
        return None;
    }
    let tw = target.clamp(320, 3840);
    let (pw, ph) = if w > tw { (tw, (h * tw / w).max(1)) } else { (w, h) };
    let mut out = vec![0u8; pw * ph * 4];
    scaler.scale(frame, w, w, h, &mut out, pw, pw, ph);
    // BGRA -> RGBA, opaco
    for px in out.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    Some((pw as u32, ph as u32, out))
}

/// Qualidade pedida pelo diálogo Transmitir.
#[derive(Clone, Debug, PartialEq)]
pub struct Quality {
    /// Altura final em pixels; `None` = a da fonte. Nunca amplia além da fonte.
    pub height: Option<u32>,
    pub fps: u32,
    pub kbps: u32,
}

/// Nomes dos monitores com dimensões, ex.: "DP-1 (1920x1080)".
pub fn monitor_names() -> Vec<String> {
    crate::capture::list_monitors()
        .iter()
        .map(|m| format!("{} ({}x{})", m.title, m.width, m.height))
        .collect()
}

/// Tamanho final do vídeo para uma fonte `sw`x`sh` e a altura pedida.
/// H.264 exige dimensões pares: reduz mantendo a proporção (largura arredondada
/// para par) e, sem redução, apenas corta o pixel ímpar que sobrar.
pub fn output_size(sw: u32, sh: u32, height: Option<u32>) -> (u32, u32) {
    let (sw, sh) = (sw.max(2), sh.max(2));
    match height {
        Some(th) if th >= 2 && th < sh => {
            let h = th & !1;
            let w = ((((sw as u64 * h as u64 + sh as u64) / (2 * sh as u64)) as u32) * 2).max(2); // par mais próximo
            (w, h)
        }
        _ => (sw & !1, sh & !1),
    }
}

/// Resumo do diálogo: "1920x1080 · 60 fps · H.264 NVENC · ~9 Mbps".
pub fn describe(source_w: u32, source_h: u32, q: &Quality) -> String {
    describe_with_encoder(source_w, source_h, q, "NVENC")
}

/// Como `describe`, com o nome do codificador ("NVENC" ou "CPU").
pub fn describe_with_encoder(source_w: u32, source_h: u32, q: &Quality, encoder: &str) -> String {
    let (w, h) = output_size(source_w, source_h, q.height);
    let mbps = if q.kbps % 1000 == 0 {
        format!("{}", q.kbps / 1000)
    } else {
        format!("{:.1}", q.kbps as f64 / 1000.0).replace('.', ",")
    };
    format!("{w}x{h} · {} fps · H.264 {encoder} · ~{mbps} Mbps", q.fps)
}

/// Bitrate inicial sugerido (kbps) para altura `h` (16:9) e `fps`.
///
/// Tabela de partida (exata nestes pontos):
///
/// | formato | kbps |
/// |---|---|
/// | 480p30 | 1500 |
/// | 720p30 | 3000 |
/// | 720p60 | 5000 |
/// | 1080p30 | 5000 |
/// | 1080p60 | 9000 |
/// | 1440p60 | 15000 |
///
/// Entre os pontos interpola linearmente pela taxa de pixels (largura 16:9 x h x
/// fps); abaixo de 480p30 e acima de 1440p60 é proporcional a pixels x fps
/// (mínimo 500 kbps).
pub fn suggested_kbps(h: u32, fps: u32) -> u32 {
    // (megapixels por segundo em 16:9, kbps), em ordem crescente.
    const T: [(u32, u32, u32); 6] =
        [(480, 30, 2000), (720, 30, 4000), (720, 60, 6000), (1080, 30, 8000), (1080, 60, 12000), (1440, 60, 20000)];
    let rate = |h: u32, fps: u32| -> f64 { (h as f64 * 16.0 / 9.0) * h as f64 * fps as f64 };
    let r = rate(h.max(1), fps.max(1));
    let pts: Vec<(f64, f64)> = T.iter().map(|&(h, f, k)| (rate(h, f), k as f64)).collect();
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let k = if r <= first.0 {
        first.1 * r / first.0
    } else if r >= last.0 {
        last.1 * r / last.0
    } else {
        let i = pts.iter().position(|p| p.0 >= r).unwrap_or(pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        a.1 + (b.1 - a.1) * (r - a.0) / (b.0 - a.0)
    };
    (k.round() as u32).max(500)
}

/// Compatibilidade: transmite o monitor inteiro na resolução da fonte.
pub fn start(url: String, monitor: usize, fps: u32, kbps: u32) -> Handle {
    start_source(url, Source::Monitor(monitor), Quality { height: None, fps, kbps }, false, None)
}

pub fn start_source(url: String, source: Source, q: Quality, share_system_audio: bool, audio_output: Option<String>) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("conectando…".to_string()));
    let preview: Preview = Arc::new(Mutex::new(None));
    let preview_width = Arc::new(std::sync::atomic::AtomicU32::new(1280));
    let (s, st, pv, pw) = (stop.clone(), status.clone(), preview.clone(), preview_width.clone());
    std::thread::spawn(move || {
        let msg = match run(&url, &source, &q, share_system_audio, audio_output.as_deref(), &s, &st, &pv, &pw) {
            Ok(()) => "parado".to_string(),
            Err(e) => format!("erro: {e:#}"),
        };
        *st.lock().unwrap() = msg;
    });
    Handle { stop, status, preview, preview_width }
}

/// Quanto tempo uma janela pode ficar indisponível (minimizada, em outra área de
/// trabalho) antes de a transmissão terminar.
const WINDOW_GRACE: Duration = Duration::from_secs(5);

/// Converte o quadro capturado no tamanho final `ow`x`oh` (pares).
/// `base` é o tamanho da fonte no início: se a janela mudou de tamanho depois,
/// o quadro novo é reduzido para caber, centralizado sobre fundo preto.
fn make_frame(raw: Raw, base: (u32, u32), (ow, oh): (u32, u32), sc: &mut Scaler) -> Arc<Vec<u8>> {
    let (rw, rh) = (raw.w as usize, raw.h as usize);
    let (ow, oh) = (ow as usize, oh as usize);
    if (raw.w, raw.h) == base {
        if rw == ow && rh == oh {
            return raw.data;
        }
        let mut out = vec![0u8; ow * oh * 4];
        if rw == ow && rh >= oh {
            // Só sobra linha ímpar: basta truncar.
            let mut d = raw.data.as_ref().clone();
            d.truncate(ow * oh * 4);
            return Arc::new(d);
        }
        // Redução (ou recorte de coluna ímpar). Recorte: região = tamanho final.
        let (cw, ch) = if rw.abs_diff(ow) <= 1 && rh.abs_diff(oh) <= 1 { (ow.min(rw), oh.min(rh)) } else { (rw, rh) };
        sc.scale(raw.data.as_slice(), rw, cw, ch, &mut out, ow, ow, oh);
        return Arc::new(out);
    }
    // Janela redimensionada no meio da transmissão: encaixa com proporção mantida.
    let s = (ow as f64 / rw as f64).min(oh as f64 / rh as f64).min(1.0);
    let fw = (((rw as f64 * s) as usize) & !1).clamp(2, ow);
    let fh = (((rh as f64 * s) as usize) & !1).clamp(2, oh);
    let mut out = vec![0u8; ow * oh * 4];
    let (x0, y0) = ((ow - fw) / 2 & !1, (oh - fh) / 2 & !1);
    sc.scale(raw.data.as_slice(), rw, rw, rh, &mut out[(y0 * ow + x0) * 4..], ow, fw, fh);
    Arc::new(out)
}

fn run(
    url: &str,
    source: &Source,
    q: &Quality,
    share_system_audio: bool,
    audio_output: Option<&str>,
    stop: &AtomicBool,
    status: &Mutex<String>,
    preview: &Preview,
    preview_width: &Arc<std::sync::atomic::AtomicU32>,
) -> anyhow::Result<()> {
    let fps = q.fps.clamp(1, 240);
    let kbps = q.kbps;
    let grabber = Grabber::new(source)?;
    let base = grabber.size()?;
    let (w, h) = output_size(base.0, base.1, q.height);
    let (w, h) = (w as usize, h as usize);
    let mut ws = crate::net::connect_ws(url)?;
    let stream_start = Instant::now();
    let mut audio_capture = share_system_audio.then(|| crate::system_audio::Capture::start(audio_output.map(str::to_owned), stream_start));

    // Captura e codificação em threads separadas: o fps é limitado pelo
    // mais lento dos dois, não pela soma.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Arc<Vec<u8>>>(1);
    let cap_stop = Arc::new(AtomicBool::new(false));
    let cs = cap_stop.clone();
    let (fail_tx, fail_rx) = std::sync::mpsc::channel::<String>();
    // A prévia é reduzida numa thread própria (até ~20 por segundo), para não roubar tempo da captura a 60 fps.
    let (ptx, prx) = std::sync::mpsc::sync_channel::<(Arc<Vec<u8>>, usize, usize)>(1);
    {
        let (preview, pw, stop_p) = (preview.clone(), preview_width.clone(), cap_stop.clone());
        std::thread::spawn(move || {
            let mut sc = Scaler::default();
            while !stop_p.load(Ordering::Relaxed) {
                match prx.recv_timeout(Duration::from_millis(250)) {
                    Ok((frame, w, h)) => {
                        let target = pw.load(Ordering::Relaxed) as usize;
                        if let Some(p) = make_preview(frame.as_slice(), w, h, target, &mut sc) {
                            if let Ok(mut g) = preview.lock() {
                                *g = Some(p);
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        });
    }
    let capture = std::thread::spawn(move || -> anyhow::Result<()> {
        let period = Duration::from_secs_f64(1.0 / fps as f64);
        let mut scaler = Scaler::default();
        let mut down_since: Option<Instant> = None;
        let mut last_preview = Instant::now() - Duration::from_secs(1);
        while !cs.load(Ordering::Relaxed) {
            let tick = Instant::now();
            match grabber.grab() {
                Ok(raw) => {
                    down_since = None;
                    let frame = make_frame(raw, base, (w as u32, h as u32), &mut scaler);
                    // prévia da minha própria tela: uma cópia a cada ~33 ms (30 por segundo, mesmo com a transmissão a 120) vai à thread que a reduz
                    if last_preview.elapsed() >= Duration::from_millis(33) {
                        last_preview = Instant::now();
                        let _ = ptx.try_send((frame.clone(), w, h));
                    }
                    // Se o encoder ainda está ocupado, descarta o quadro.
                    let _ = tx.try_send(frame);
                }
                Err(e) if grabber.is_window() => {
                    // Janela minimizada/trocada de área: espera um pouco antes de desistir.
                    let t = *down_since.get_or_insert(tick);
                    if t.elapsed() > WINDOW_GRACE {
                        return Err(e.context("a transmissão da janela terminou"));
                    }
                    let _ = fail_tx.send(format!("janela indisponível ({e:#}), aguardando…"));
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
                Err(e) => return Err(e),
            }
            if let Some(rest) = period.checked_sub(tick.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        Ok(())
    });

    let (mut sent, mut t0) = (0u32, Instant::now());
    let mut recv = || -> Option<Arc<Vec<u8>>> {
        while !stop.load(Ordering::Relaxed) {
            while let Ok(m) = fail_rx.try_recv() {
                *status.lock().unwrap() = m;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(d) => return Some(d),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return None, // captura terminou (erro)
            }
        }
        None
    };
    let mut audio_pending = std::collections::VecDeque::<(u64, Vec<u8>)>::new();
    let mut audio_error: Option<String> = None;
    let mut last_media_pts = 0u64;
    let mut send = |key: bool, h264: &[u8]| -> anyhow::Result<()> {
        if let Some(capture) = audio_capture.as_mut() {
            loop {
                match capture.rx.try_recv() {
                    Ok(crate::system_audio::Event::Packet { pts_us, opus }) => audio_pending.push_back((pts_us, opus)),
                    Ok(crate::system_audio::Event::Failed(e)) => audio_error = Some(e),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        audio_error.get_or_insert_with(|| "captura encerrada".into());
                        break;
                    }
                }
            }
        }
        let video_pts = stream_start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        while audio_pending.front().is_some_and(|(pts, _)| *pts <= video_pts) {
            let (pts, opus) = audio_pending.pop_front().unwrap();
            if pts <= last_media_pts { continue; }
            let packet = telacast_proto::encode_media_packet(telacast_proto::MediaKind::SystemAudioOpus, pts, &opus)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            ws.send(Message::Binary(packet.into()))?;
            last_media_pts = pts;
        }
        let video_pts = video_pts.max(last_media_pts);
        let kind = if key { telacast_proto::MediaKind::H264Keyframe } else { telacast_proto::MediaKind::H264Delta };
        let payload = telacast_proto::encode_media_packet(kind, video_pts, h264).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        ws.send(Message::Binary(payload.into()))?;
        last_media_pts = video_pts;
        sent += 1;
        if t0.elapsed() >= Duration::from_secs(1) {
            let audio_suffix = audio_error.as_deref().map(|e| format!(" · áudio indisponível: {e}")).unwrap_or_default();
            *status.lock().unwrap() = format!("transmitindo · {sent} fps · {w}×{h}{audio_suffix}");
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
                .usage_type(UsageType::ScreenContentRealTime)
                .num_threads(4);
            let mut enc = Encoder::with_api_config(OpenH264API::from_source(), cfg)?;
            (|| -> anyhow::Result<()> {
                while let Some(data) = recv() {
                    let yuv = YUVBuffer::from_bgra8_source(BgraSliceU8::new(data.as_slice(), (w, h)));
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
    let g = Grabber::new(&Source::Monitor(0))?;
    let (bw, bh) = g.size()?;
    let (w, h) = output_size(bw, bh, None);
    let (w, h) = (w as u16, h as u16);
    let mut sc = Scaler::default();
    let mut grab = || -> anyhow::Result<Arc<Vec<u8>>> {
        Ok(make_frame(g.grab()?, (bw, bh), (w as u32, h as u32), &mut sc))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tamanho_final() {
        assert_eq!(output_size(1920, 1080, None), (1920, 1080));
        assert_eq!(output_size(1920, 1080, Some(720)), (1280, 720));
        assert_eq!(output_size(1920, 1080, Some(480)), (854, 480));
        assert_eq!(output_size(1920, 1080, Some(1440)), (1920, 1080)); // nunca amplia
        assert_eq!(output_size(1920, 1080, Some(1080)), (1920, 1080));
        assert_eq!(output_size(2560, 1440, Some(1080)), (1920, 1080));
        assert_eq!(output_size(1367, 769, None), (1366, 768)); // ímpar vira par
        assert_eq!(output_size(1366, 768, Some(721)), (1280, 720)); // altura pedida ímpar
        let (w, h) = output_size(3441, 1440, Some(720));
        assert!(w % 2 == 0 && h % 2 == 0 && h == 720);
        assert_eq!(output_size(0, 0, Some(720)), (2, 2));
    }

    #[test]
    fn resumo() {
        let q = Quality { height: None, fps: 60, kbps: 9000 };
        assert_eq!(describe(1920, 1080, &q), "1920x1080 · 60 fps · H.264 NVENC · ~9 Mbps");
        let q = Quality { height: Some(720), fps: 30, kbps: 1500 };
        assert_eq!(describe(1920, 1080, &q), "1280x720 · 30 fps · H.264 NVENC · ~1,5 Mbps");
        assert_eq!(describe_with_encoder(1920, 1080, &q, "CPU"), "1280x720 · 30 fps · H.264 CPU · ~1,5 Mbps");
    }

    #[test]
    fn tabela_de_bitrate() {
        assert_eq!(suggested_kbps(480, 30), 2000);
        assert_eq!(suggested_kbps(720, 30), 4000);
        assert_eq!(suggested_kbps(720, 60), 6000);
        assert_eq!(suggested_kbps(1080, 30), 8000);
        assert_eq!(suggested_kbps(1080, 60), 12000);
        assert_eq!(suggested_kbps(1440, 60), 20000);
        // monotônica na taxa de pixels
        assert!(suggested_kbps(1080, 45) > 8000 && suggested_kbps(1080, 45) < 12000);
        assert!(suggested_kbps(480, 15) < 1500 && suggested_kbps(480, 15) >= 500);
        assert!(suggested_kbps(2160, 60) > 15000);
        assert!(suggested_kbps(1440, 144) > suggested_kbps(1440, 60));
    }

    fn raw(w: u32, h: u32, v: u8) -> Raw {
        Raw { w, h, data: Arc::new(vec![v; (w * h * 4) as usize]) }
    }

    #[test]
    fn quadro_sem_escala_reaproveita_o_buffer() {
        let mut sc = Scaler::default();
        let f = make_frame(raw(64, 36, 9), (64, 36), (64, 36), &mut sc);
        assert_eq!(f.len(), 64 * 36 * 4);
    }

    #[test]
    fn quadro_reduzido_e_recortado() {
        let mut sc = Scaler::default();
        let f = make_frame(raw(1920, 1080, 50), (1920, 1080), (1280, 720), &mut sc);
        assert_eq!(f.len(), 1280 * 720 * 4);
        assert!(f.iter().all(|&b| b == 50));
        // largura ímpar: recorta uma coluna
        let f = make_frame(raw(101, 60, 7), (101, 60), (100, 60), &mut sc);
        assert_eq!(f.len(), 100 * 60 * 4);
        assert!(f.iter().all(|&b| b == 7));
        // altura ímpar: trunca
        let f = make_frame(raw(100, 61, 7), (100, 61), (100, 60), &mut sc);
        assert_eq!(f.len(), 100 * 60 * 4);
    }

    #[test]
    fn janela_redimensionada_vira_letterbox() {
        let mut sc = Scaler::default();
        // base 200x100, a janela encolheu para 100x100: cabe sem ampliar, com bordas pretas.
        let f = make_frame(raw(100, 100, 255), (200, 100), (200, 100), &mut sc);
        assert_eq!(f.len(), 200 * 100 * 4);
        assert_eq!(f[0], 0); // canto esquerdo é borda
        assert_eq!(f[(50 * 200 + 100) * 4], 255); // centro é a janela
        // cresceu para 400x200: reduz para caber
        let f = make_frame(raw(400, 200, 255), (200, 100), (200, 100), &mut sc);
        assert_eq!(f.len(), 200 * 100 * 4);
        assert!(f.iter().all(|&b| b == 255));
    }
}

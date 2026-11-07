use eframe::egui::{ColorImage, Context};
use openh264::{decoder::Decoder, formats::YUVSource};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tungstenite::Message;

pub struct Handle {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub frame: Arc<Mutex<Option<ColorImage>>>,
}

pub fn start(url: String, ctx: Context) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("conectando…".to_string()));
    let frame = Arc::new(Mutex::new(None));
    let (s, st, f) = (stop.clone(), status.clone(), frame.clone());
    std::thread::spawn(move || {
        let msg = match run(&url, &s, &st, &f, &ctx) {
            Ok(()) => "desconectado".to_string(),
            Err(e) => format!("erro: {e:#}"),
        };
        *st.lock().unwrap() = msg;
        ctx.request_repaint();
    });
    Handle { stop, status, frame }
}

fn run(
    url: &str,
    stop: &AtomicBool,
    status: &Mutex<String>,
    out: &Mutex<Option<ColorImage>>,
    ctx: &Context,
) -> anyhow::Result<()> {
    let mut ws = crate::net::connect_ws(url)?;
    // Timeout de leitura para poder checar `stop`.
    crate::net::set_read_timeout(&mut ws, Duration::from_millis(200))?;
    let mut dec = Decoder::new()?;
    *status.lock().unwrap() = "assistindo".into();
    while !stop.load(Ordering::Relaxed) {
        let msg = match ws.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(e) => return Err(e.into()),
        };
        match msg {
            Message::Binary(data) if data.len() > 1 => {
                if let Ok(Some(yuv)) = dec.decode(&data[1..]) {
                    let (w, h) = yuv.dimensions();
                    let mut rgba = vec![0u8; w * h * 4];
                    yuv.write_rgba8(&mut rgba);
                    *out.lock().unwrap() = Some(ColorImage::from_rgba_unmultiplied([w, h], &rgba));
                    ctx.request_repaint();
                }
            }
            Message::Text(t) => anyhow::bail!("{t}"),
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

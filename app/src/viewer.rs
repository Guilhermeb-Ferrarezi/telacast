use eframe::egui::{ColorImage, Context};
use openh264::{decoder::Decoder, formats::YUVSource};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::Message;

pub struct Handle {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub frame: Arc<Mutex<Option<ColorImage>>>,
    pub audio_muted: Arc<AtomicBool>,
}

pub fn start(url: String, ctx: Context) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("conectando…".to_string()));
    let frame = Arc::new(Mutex::new(None));
    let audio_muted = Arc::new(AtomicBool::new(false));
    let (s, st, f, m) = (stop.clone(), status.clone(), frame.clone(), audio_muted.clone());
    std::thread::spawn(move || {
        let msg = match run(&url, &s, &st, &f, &m, &ctx) {
            Ok(()) => "desconectado".to_string(),
            Err(e) => format!("erro: {e:#}"),
        };
        *st.lock().unwrap() = msg;
        ctx.request_repaint();
    });
    Handle { stop, status, frame, audio_muted }
}

fn run(
    url: &str,
    stop: &AtomicBool,
    status: &Mutex<String>,
    out: &Mutex<Option<ColorImage>>,
    audio_muted: &Arc<AtomicBool>,
    ctx: &Context,
) -> anyhow::Result<()> {
    let mut ws = crate::net::connect_ws(url)?;
    // Timeout de leitura para poder checar `stop`.
    crate::net::set_read_timeout(&mut ws, Duration::from_millis(200))?;
    let mut dec = Decoder::new()?;
    let mut player = None;
    let mut timeline: Option<(Instant, u64)> = None;
    let mut pending_video = VecDeque::<(Instant, ColorImage)>::new();
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
                flush_video(&mut pending_video, out, ctx);
                continue
            }
            Err(e) => return Err(e.into()),
        };
        match msg {
            Message::Binary(data) => {
                if let Ok(packet) = telacast_proto::parse_media_packet(&data) {
                    let (anchor, base_pts) = *timeline.get_or_insert_with(|| (Instant::now(), packet.pts_us));
                    match packet.kind {
                        telacast_proto::MediaKind::H264Delta | telacast_proto::MediaKind::H264Keyframe => {
                            if let Ok(Some(yuv)) = dec.decode(packet.payload) {
                                let (w, h) = yuv.dimensions();
                                let mut rgba = vec![0u8; w * h * 4];
                                yuv.write_rgba8(&mut rgba);
                                let image = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                                let due = anchor + Duration::from_micros(packet.pts_us.saturating_sub(base_pts).min(1_000_000)) + Duration::from_millis(60);
                                pending_video.push_back((due, image));
                                while pending_video.len() > 32 { pending_video.pop_front(); }
                            }
                        }
                        telacast_proto::MediaKind::SystemAudioOpus => {
                            if player.is_none() {
                                player = crate::system_audio_playback::Player::new(audio_muted.clone(), anchor, base_pts).ok();
                            }
                            if let Some(player) = player.as_mut() {
                                player.mute(audio_muted.load(Ordering::Relaxed));
                                let _ = player.push(packet.pts_us, packet.payload);
                            }
                        }
                    }
                }
            }
            Message::Text(t) => anyhow::bail!("{t}"),
            Message::Close(_) => break,
            _ => {}
        }
        flush_video(&mut pending_video, out, ctx);
    }
    Ok(())
}

fn flush_video(pending: &mut VecDeque<(Instant, ColorImage)>, out: &Mutex<Option<ColorImage>>, ctx: &Context) {
    let now = Instant::now();
    let mut newest = None;
    while pending.front().is_some_and(|(due, _)| *due <= now) {
        newest = pending.pop_front().map(|(_, image)| image);
    }
    if let Some(image) = newest {
        *out.lock().unwrap() = Some(image);
        ctx.request_repaint();
    }
}

use eframe::egui::{ColorImage, Context};
use openh264::{decoder::Decoder, formats::YUVSource};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::Message;

const MAX_VIDEO_BACKLOG_US: u64 = 250_000;
const PRESENTATION_DELAY: Duration = Duration::from_millis(60);

pub struct Handle {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub frame: Arc<Mutex<Option<ColorImage>>>,
    pub audio_muted: Arc<AtomicBool>,
    /// Volume da faixa de áudio do sistema recebida (0–100%).
    pub audio_volume: Arc<AtomicU32>,
}

pub fn start(url: String, ctx: Context) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("conectando…".to_string()));
    let frame = Arc::new(Mutex::new(None));
    let audio_muted = Arc::new(AtomicBool::new(false));
    let audio_volume = Arc::new(AtomicU32::new(100));
    let (s, st, f, m, v) = (stop.clone(), status.clone(), frame.clone(), audio_muted.clone(), audio_volume.clone());
    std::thread::spawn(move || {
        let msg = match run(&url, &s, &st, &f, &m, &v, &ctx) {
            Ok(()) => "desconectado".to_string(),
            Err(e) => format!("erro: {e:#}"),
        };
        // Não mantenha o último quadro visível depois que a call ou stream termina.
        *f.lock().unwrap() = None;
        *st.lock().unwrap() = msg;
        ctx.request_repaint();
    });
    Handle { stop, status, frame, audio_muted, audio_volume }
}

fn run(
    url: &str,
    stop: &AtomicBool,
    status: &Mutex<String>,
    out: &Mutex<Option<ColorImage>>,
    audio_muted: &Arc<AtomicBool>,
    audio_volume: &Arc<AtomicU32>,
    ctx: &Context,
) -> anyhow::Result<()> {
    let mut ws = crate::net::connect_ws(url)?;
    // Timeout de leitura para poder checar `stop`.
    crate::net::set_read_timeout(&mut ws, Duration::from_millis(200))?;
    let mut dec = Decoder::new()?;
    let mut player = None;
    let mut timeline: Option<(Instant, u64)> = None;
    let mut pending_video = VecDeque::<(Instant, ColorImage)>::new();
    let mut waiting_for_keyframe = false;
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
                            let local_elapsed = anchor.elapsed().as_micros().min(u64::MAX as u128) as u64;
                            let media_elapsed = packet.pts_us.saturating_sub(base_pts);
                            if media_elapsed.saturating_sub(local_elapsed) > MAX_VIDEO_BACKLOG_US {
                                // Se o espectador acumular quadros no TCP, descarte os dependentes
                                // até um quadro-chave permitir saltar de volta para o ao vivo.
                                waiting_for_keyframe = true;
                                pending_video.clear();
                            }
                            if waiting_for_keyframe {
                                if packet.kind != telacast_proto::MediaKind::H264Keyframe {
                                    continue;
                                }
                                dec = Decoder::new()?;
                                timeline = Some((Instant::now(), packet.pts_us));
                                waiting_for_keyframe = false;
                            }
                            if let Ok(Some(yuv)) = dec.decode(packet.payload) {
                                let (w, h) = yuv.dimensions();
                                let mut rgba = vec![0u8; w * h * 4];
                                yuv.write_rgba8(&mut rgba);
                                let image = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                                let (anchor, base_pts) = timeline.unwrap_or((anchor, base_pts));
                                let due = anchor + Duration::from_micros(packet.pts_us.saturating_sub(base_pts).min(1_000_000)) + PRESENTATION_DELAY;
                                pending_video.push_back((due, image));
                                while pending_video.len() > 32 { pending_video.pop_front(); }
                            }
                        }
                        telacast_proto::MediaKind::SystemAudioOpus => {
                            if player.is_none() {
                                player = crate::system_audio_playback::Player::new(audio_muted.clone(), audio_volume.clone(), anchor, base_pts).ok();
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

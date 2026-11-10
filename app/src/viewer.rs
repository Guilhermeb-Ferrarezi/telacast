use eframe::egui::{ColorImage, Context};
use openh264::{decoder::Decoder, formats::YUVSource};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{sync_channel, TrySendError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::Message;

const MAX_VIDEO_BACKLOG_US: u64 = 250_000;
const PRESENTATION_DELAY: Duration = Duration::from_millis(120);
// 32 pacotes absorvem picos curtos a 144 FPS (~220 ms) sem uma fila longa.
const INCOMING_PACKET_QUEUE: usize = 32;

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
    let (s, st, f, m, v) = (
        stop.clone(),
        status.clone(),
        frame.clone(),
        audio_muted.clone(),
        audio_volume.clone(),
    );
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
    Handle {
        stop,
        status,
        frame,
        audio_muted,
        audio_volume,
    }
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
    // A leitura da rede não pode esperar o decodificador. Uma fila curta impede
    // que o TCP acumule muitos quadros antigos quando o espectador fica ocupado.
    let (packet_tx, packet_rx) = sync_channel::<anyhow::Result<Message>>(INCOMING_PACKET_QUEUE);
    let reader_stop = Arc::new(AtomicBool::new(false));
    let reader_stop_thread = reader_stop.clone();
    let stream_stop = Arc::new(AtomicBool::new(false));
    let stream_stop_thread = stream_stop.clone();
    let force_keyframe = Arc::new(AtomicBool::new(false));
    let force_keyframe_reader = force_keyframe.clone();
    let reader_audio_muted = audio_muted.clone();
    let reader_audio_volume = audio_volume.clone();
    let reader = std::thread::spawn(move || {
        let mut dropping_until_keyframe = false;
        let mut audio_player = None;
        let mut audio_timeline: Option<(Instant, u64)> = None;
        while !reader_stop_thread.load(Ordering::Relaxed)
            && !stream_stop_thread.load(Ordering::Relaxed)
        {
            match ws.read() {
                Ok(Message::Binary(data)) => {
                    let packet = telacast_proto::parse_media_packet(&data).ok();
                    let kind = packet.as_ref().map(|p| p.kind);
                    // Áudio não entra na fila do decoder H.264. Assim, uma fila de
                    // vídeo cheia (ou um quadro caro de decodificar) não descarta
                    // os pacotes Opus junto com os quadros até o próximo keyframe.
                    if let Some(packet) = packet.filter(|p| p.kind == telacast_proto::MediaKind::SystemAudioOpus) {
                        let (anchor, base_pts) = *audio_timeline.get_or_insert_with(|| (Instant::now(), packet.pts_us));
                        if audio_player.is_none() {
                            audio_player = crate::system_audio_playback::Player::new(
                                reader_audio_muted.clone(),
                                reader_audio_volume.clone(),
                                anchor,
                                base_pts,
                            ).ok();
                        }
                        if let Some(player) = audio_player.as_mut() {
                            player.mute(reader_audio_muted.load(Ordering::Relaxed));
                            let _ = player.push(packet.pts_us, packet.payload);
                        }
                        continue;
                    }
                    if dropping_until_keyframe {
                        if kind != Some(telacast_proto::MediaKind::H264Keyframe) {
                            continue;
                        }
                        match packet_tx.try_send(Ok(Message::Binary(data))) {
                            Ok(()) => dropping_until_keyframe = false,
                            Err(TrySendError::Full(_)) => {
                                force_keyframe_reader.store(true, Ordering::Relaxed);
                            }
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                    } else {
                        match packet_tx.try_send(Ok(Message::Binary(data))) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                dropping_until_keyframe = true;
                                force_keyframe_reader.store(true, Ordering::Relaxed);
                            }
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                    }
                }
                Ok(Message::Text(message)) => {
                    let _ = packet_tx.try_send(Err(anyhow::anyhow!(message.to_string())));
                    break;
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => {
                    let _ = packet_tx.try_send(Err(e.into()));
                    break;
                }
            }
        }
    });
    let _reader_guard = ReaderStop {
        stop: reader_stop,
        stream_stop,
        reader: Some(reader),
    };
    let mut dec = Decoder::new()?;
    let mut timeline: Option<(Instant, u64)> = None;
    let mut pending_video = VecDeque::<(Instant, ColorImage)>::new();
    let mut waiting_for_keyframe = false;
    *status.lock().unwrap() = "assistindo".into();
    while !stop.load(Ordering::Relaxed) {
        let message = match packet_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(message)) => message,
            Ok(Err(e)) => return Err(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                flush_video(&mut pending_video, out, ctx);
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if force_keyframe.swap(false, Ordering::Relaxed) {
            waiting_for_keyframe = true;
            pending_video.clear();
        }
        if let Message::Binary(data) = message {
            if let Ok(packet) = telacast_proto::parse_media_packet(&data) {
                let (anchor, base_pts) =
                    *timeline.get_or_insert_with(|| (Instant::now(), packet.pts_us));
                match packet.kind {
                    telacast_proto::MediaKind::H264Delta
                    | telacast_proto::MediaKind::H264Keyframe => {
                        let local_elapsed =
                            anchor.elapsed().as_micros().min(u64::MAX as u128) as u64;
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
                            let due = anchor
                                + Duration::from_micros(
                                    packet.pts_us.saturating_sub(base_pts).min(1_000_000),
                                )
                                + PRESENTATION_DELAY;
                            pending_video.push_back((due, image));
                            while pending_video.len() > 32 {
                                pending_video.pop_front();
                            }
                        }
                    }
                    telacast_proto::MediaKind::SystemAudioOpus => {}
                }
            }
        }
        flush_video(&mut pending_video, out, ctx);
    }
    Ok(())
}

struct ReaderStop {
    stop: Arc<AtomicBool>,
    stream_stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ReaderStop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stream_stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn flush_video(
    pending: &mut VecDeque<(Instant, ColorImage)>,
    out: &Mutex<Option<ColorImage>>,
    ctx: &Context,
) {
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

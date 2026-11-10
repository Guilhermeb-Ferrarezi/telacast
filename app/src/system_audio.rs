//! Captura da saída do sistema para a faixa Opus da transmissão de tela.
use std::{
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const FRAME_SAMPLES: usize = 960 * CHANNELS;
const OPUS_MAX_PACKET: usize = 4_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOutput {
    pub id: String,
    pub name: String,
}

pub enum Event {
    Packet { pts_us: u64, opus: Vec<u8> },
    Failed(String),
}

pub struct Capture {
    pub rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn start(output: Option<String>, clock_start: Instant) -> Self {
        let (tx, rx) = mpsc::sync_channel(25); // no máximo 500 ms de áudio codificado
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        let (worker_stop, worker_child) = (stop.clone(), child.clone());
        let thread = thread::Builder::new()
            .name("telacast-system-audio-capture".into())
            .spawn(move || {
                if let Err(e) = capture_worker(output.as_deref(), &clock_start, &worker_stop, &worker_child, tx.clone()) {
                    let _ = tx.try_send(Event::Failed(e));
                }
            })
            .ok();
        Self { rx, stop, child, thread }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn capture_worker(
    output: Option<&str>,
    clock_start: &Instant,
    stop: &AtomicBool,
    child: &Arc<Mutex<Option<Child>>>,
    tx: SyncSender<Event>,
) -> Result<(), String> {
    let mut encoder = opus::Encoder::new(RATE, opus::Channels::Stereo, opus::Application::Audio).map_err(|e| e.to_string())?;
    let _ = encoder.set_bitrate(opus::Bitrate::Bits(128_000));
    let _ = encoder.set_vbr(true);
    let mut pcm = Vec::with_capacity(FRAME_SAMPLES * 4);
    let mut packet = vec![0u8; OPUS_MAX_PACKET];
    let mut last_pts = 0u64;
    let mut consume = |samples: &[f32]| -> Result<(), String> {
        pcm.extend(samples.iter().map(|x| if x.is_finite() { x.clamp(-1.0, 1.0) } else { 0.0 }));
        while pcm.len() >= FRAME_SAMPLES && !stop.load(Ordering::Relaxed) {
            let end = clock_start.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let pts_us = last_pts.max(end.saturating_sub(20_000));
            let n = encoder.encode_float(&pcm[..FRAME_SAMPLES], &mut packet).map_err(|e| format!("codificação Opus: {e}"))?;
            if n > 0 {
                let event = Event::Packet { pts_us, opus: packet[..n].to_vec() };
                match tx.try_send(event) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => return Err("transmissão encerrada".into()),
                }
            }
            last_pts = pts_us.saturating_add(20_000);
            pcm.drain(..FRAME_SAMPLES);
        }
        Ok(())
    };

    #[cfg(windows)]
    let result = system_audio_windows::run(output, stop, child, &mut consume);
    #[cfg(target_os = "linux")]
    let result = system_audio_linux::run(output, stop, child, &mut consume);
    #[cfg(not(any(windows, target_os = "linux")))]
    let result: Result<(), String> = Err("captura do áudio do sistema não é suportada nesta plataforma".into());
    if let Ok(mut slot) = child.lock() {
        if let Some(mut child) = slot.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    result
}

#[cfg(windows)]
mod system_audio_windows;
#[cfg(target_os = "linux")]
mod system_audio_linux;

pub fn outputs() -> Vec<AudioOutput> {
    #[cfg(windows)]
    { return system_audio_windows::outputs(); }
    #[cfg(target_os = "linux")]
    { return system_audio_linux::outputs(); }
    #[allow(unreachable_code)]
    Vec::new()
}

pub fn unavailable_hint() -> &'static str {
    #[cfg(windows)]
    { "Nenhuma saída de áudio foi encontrada no Windows." }
    #[cfg(target_os = "linux")]
    { "A captura de áudio requer PipeWire com pw-cat instalado." }
    #[cfg(not(any(windows, target_os = "linux")))]
    { "A captura de áudio não está disponível nesta plataforma." }
}

pub fn capture_note() -> &'static str {
    #[cfg(windows)]
    { "Em versões recentes do Windows, a captura inclui os apps elegíveis em todos os dispositivos e exclui o Telacast; em versões antigas, usa o dispositivo escolhido e pode incluir a voz reproduzida nele." }
    #[cfg(target_os = "linux")]
    { "O monitor PipeWire inclui a reprodução da saída escolhida, inclusive a voz do Telacast; use outra saída para a voz se notar eco." }
    #[cfg(not(any(windows, target_os = "linux")))]
    { "" }
}


use super::AudioOutput;
use std::{
    collections::VecDeque,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    thread,
    time::Duration,
};
use wasapi::{initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

pub fn outputs() -> Vec<AudioOutput> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = thread::Builder::new().name("telacast-audio-devices".into()).spawn(move || {
        let result = (|| -> Result<Vec<AudioOutput>, String> {
            initialize_mta().ok().map_err(|e| e.to_string())?;
            let en = DeviceEnumerator::new().map_err(|e| e.to_string())?;
            let device = en.get_default_device(Direction::Render).map_err(|e| e.to_string())?;
            Ok(vec![AudioOutput {
                id: device.get_id().map_err(|e| e.to_string())?,
                name: device.get_friendlyname().map_err(|e| e.to_string())?,
            }])
        })();
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(2)).ok().and_then(Result::ok).unwrap_or_default()
}

pub fn run(
    output: Option<&str>,
    stop: &AtomicBool,
    _child: &Arc<Mutex<Option<std::process::Child>>>,
    consume: &mut impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<(), String> {
    initialize_mta().ok().map_err(|e| format!("inicialização WASAPI: {e}"))?;
    let format = WaveFormat::new(32, 32, SampleType::Float, 48_000, 2, None);
    if let Ok(mut client) = AudioClient::new_application_loopback_client(std::process::id(), false) {
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 200_000 };
        if client.initialize_client(&format, Direction::Capture, mode, None).is_ok() {
            // Windows 10 build 20348+: exclude the Telacast process tree to prevent voice echo.
            return run_client(client, stop, consume);
        }
    }
    let en = DeviceEnumerator::new().map_err(|e| format!("WASAPI: {e}"))?;
    let device = match output {
        Some(id) => en.get_device(id).map_err(|e| format!("saída de áudio indisponível: {e}"))?,
        None => en.get_default_device(Direction::Render).map_err(|e| format!("saída de áudio padrão indisponível: {e}"))?,
    };
    let mut client: AudioClient = device.get_iaudioclient().map_err(|e| format!("WASAPI: {e}"))?;
    let (_, min_period) = client.get_device_period().map_err(|e| format!("período WASAPI: {e}"))?;
    client
        .initialize_client(
            &format,
            Direction::Capture,
            StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: min_period },
            None,
        )
        .map_err(|e| format!("inicialização do loopback WASAPI: {e}"))?;
    run_client(client, stop, consume)
}

fn run_client(
    mut client: AudioClient,
    stop: &AtomicBool,
    consume: &mut impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<(), String> {
    let event = client.set_get_eventhandle().map_err(|e| format!("evento WASAPI: {e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| format!("captura WASAPI: {e}"))?;
    client.start_stream().map_err(|e| format!("início WASAPI: {e}"))?;
    let mut raw = VecDeque::<u8>::with_capacity(48_000);
    let mut samples = Vec::with_capacity(4_096);
    while !stop.load(Ordering::Relaxed) {
        match event.wait_for_event(100_000) {
            Ok(()) | Err(wasapi::WasapiError::EventTimeout) => {}
            Err(e) => return Err(format!("espera do loopback WASAPI: {e}")),
        }
        loop {
            let frames = capture.get_next_packet_size().map_err(|e| format!("pacote WASAPI: {e}"))?;
            if !matches!(frames, Some(n) if n > 0) { break; }
            raw.clear();
            capture.read_from_device_to_deque(&mut raw).map_err(|e| format!("leitura WASAPI: {e}"))?;
            samples.clear();
            for bytes in raw.make_contiguous().chunks_exact(4) {
                let sample = f32::from_le_bytes(bytes.try_into().unwrap());
                samples.push(if sample.is_finite() { sample } else { 0.0 });
            }
            if !samples.is_empty() { consume(&samples)?; }
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

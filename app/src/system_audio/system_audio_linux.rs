use super::AudioOutput;
use serde_json::Value;
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};

pub fn outputs() -> Vec<AudioOutput> {
    let Ok(out) = Command::new("pw-dump").output() else { return Vec::new() };
    if !out.status.success() { return Vec::new(); }
    let Ok(doc) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    let Some(items) = doc.as_array() else { return Vec::new() };
    let mut result = vec![AudioOutput { id: "auto".into(), name: "Saída padrão do sistema".into() }];
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Node") { continue; }
        let Some(props) = item.get("info").and_then(|v| v.get("props")) else { continue };
        if props.get("media.class").and_then(Value::as_str) != Some("Audio/Sink") { continue; }
        let Some(id) = props.get("node.name").and_then(Value::as_str) else { continue };
        let name = props.get("node.description").and_then(Value::as_str).unwrap_or(id);
        result.push(AudioOutput { id: id.to_owned(), name: name.to_owned() });
    }
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

pub fn run(
    output: Option<&str>,
    stop: &AtomicBool,
    shared_child: &Arc<Mutex<Option<Child>>>,
    consume: &mut impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<(), String> {
    let target = output.unwrap_or("auto");
    let mut child = Command::new("pw-cat")
        .args([
            "--record", "--raw", "--format", "f32", "--rate", "48000", "--channels", "2", "--target", target,
            "--properties", r#"{"stream.capture.sink":true}"#, "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("não foi possível iniciar pw-cat: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("pw-cat não abriu a saída PCM")?;
    {
        let mut slot = shared_child.lock().map_err(|_| "estado de captura indisponível")?;
        *slot = Some(child);
    }
    let mut bytes = [0u8; 16_384];
    let mut tail = Vec::<u8>::with_capacity(8);
    let mut samples = Vec::<f32>::with_capacity(4_096);
    while !stop.load(Ordering::Relaxed) {
        let n = stdout.read(&mut bytes).map_err(|e| format!("leitura do monitor PipeWire: {e}"))?;
        if n == 0 { break; }
        tail.extend_from_slice(&bytes[..n]);
        let usable = tail.len() / 4 * 4;
        samples.clear();
        for sample in tail[..usable].chunks_exact(4) {
            let value = f32::from_le_bytes(sample.try_into().unwrap());
            samples.push(if value.is_finite() { value } else { 0.0 });
        }
        if !samples.is_empty() { consume(&samples)?; }
        tail.drain(..usable);
    }
    if !stop.load(Ordering::Relaxed) {
        return Err("pw-cat encerrou a captura de áudio inesperadamente".into());
    }
    Ok(())
}

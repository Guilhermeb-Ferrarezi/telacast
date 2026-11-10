//! Toques de chamada locais; não passam pela rede nem pelo mixer da voz.
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub enum Effect {
    Incoming,
    Outgoing,
    Connected,
    Ended,
}

/// Repete o toque até a chamada conectar, ser recusada ou ser encerrada.
pub fn ring(output_name: Option<String>, effect: Effect) {
    let generation = GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    let _ = thread::Builder::new()
        .name("telacast-call-ring".into())
        .spawn(move || play_blocking(output_name.as_deref(), effect, generation, true));
}

/// Para qualquer toque pendente e toca um efeito curto uma única vez.
pub fn play(output_name: Option<String>, effect: Effect) {
    let generation = GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    let _ = thread::Builder::new()
        .name("telacast-call-sound".into())
        .spawn(move || play_blocking(output_name.as_deref(), effect, generation, false));
}

/// Cancela imediatamente o toque em repetição.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

fn play_blocking(output_name: Option<&str>, effect: Effect, generation: u64, repeat: bool) {
    let host = cpal::default_host();
    let device = output_name
        .and_then(|name| {
            host.output_devices()
                .ok()?
                .find(|device| device.name().is_ok_and(|candidate| candidate == name))
        })
        .or_else(|| host.default_output_device());
    let Some(device) = device else { return };
    let Ok(supported) = device.default_output_config() else { return };
    let config = supported.config();
    let rate = config.sample_rate.0.max(1);
    let samples = samples(effect, rate);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => stream::<f32>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::F64 => stream::<f64>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::I8 => stream::<i8>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::I16 => stream::<i16>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::I24 => stream::<cpal::I24>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::I32 => stream::<i32>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::I64 => stream::<i64>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::U8 => stream::<u8>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::U16 => stream::<u16>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::U32 => stream::<u32>(&device, &config, samples.clone(), generation, repeat),
        SampleFormat::U64 => stream::<u64>(&device, &config, samples.clone(), generation, repeat),
        _ => return,
    };
    let Ok(stream) = stream else { return };
    if stream.play().is_ok() {
        while GENERATION.load(Ordering::Acquire) == generation {
            thread::sleep(Duration::from_millis(35));
            if !repeat && samples.len() as f64 / rate as f64 <= 0.035 {
                break;
            }
            // One-shot sounds finish naturally; repeating rings live until stop().
            if !repeat {
                let elapsed = samples.len() as f64 / rate as f64;
                let mut remaining = elapsed - 0.035;
                while remaining > 0.0 && GENERATION.load(Ordering::Acquire) == generation {
                    let slice = remaining.min(0.035);
                    thread::sleep(Duration::from_secs_f64(slice));
                    remaining -= slice;
                }
                break;
            }
        }
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Vec<f32>,
    generation: u64,
    repeat: bool,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels.max(1) as usize;
    let mut frame = 0usize;
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            for channel_frame in output.chunks_mut(channels) {
                let active = GENERATION.load(Ordering::Relaxed) == generation;
                let value = if active {
                    samples.get(frame).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                for sample in channel_frame {
                    *sample = T::from_sample(value);
                }
                frame += 1;
                if repeat && frame >= samples.len() {
                    frame = 0;
                }
            }
        },
        |_error| {},
        None,
    )
}

fn samples(effect: Effect, rate: u32) -> Vec<f32> {
    let mut result = Vec::new();
    match effect {
        // Um pequeno arpejo quente, com pausas longas para não cansar enquanto toca.
        Effect::Incoming => {
            bell(&mut result, rate, 659.25, 0.34, 0.11);
            silence(&mut result, rate, 0.09);
            bell(&mut result, rate, 783.99, 0.42, 0.11);
            silence(&mut result, rate, 0.09);
            bell(&mut result, rate, 987.77, 0.56, 0.10);
            silence(&mut result, rate, 1.05);
        }
        // Ringback distinto, mais baixo e cadenciado, enquanto a outra pessoa atende.
        Effect::Outgoing => {
            dual_tone(&mut result, rate, 440.0, 554.37, 0.62, 0.075);
            silence(&mut result, rate, 0.13);
            dual_tone(&mut result, rate, 493.88, 622.25, 0.78, 0.075);
            silence(&mut result, rate, 2.15);
        }
        Effect::Connected => {
            bell(&mut result, rate, 659.25, 0.13, 0.10);
            silence(&mut result, rate, 0.035);
            bell(&mut result, rate, 987.77, 0.20, 0.10);
        }
        Effect::Ended => {
            bell(&mut result, rate, 783.99, 0.15, 0.075);
            silence(&mut result, rate, 0.045);
            bell(&mut result, rate, 587.33, 0.21, 0.075);
        }
    }
    result
}

fn bell(output: &mut Vec<f32>, rate: u32, frequency: f32, seconds: f32, volume: f32) {
    let frames = (rate as f32 * seconds) as usize;
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let attack = (i as f32 / (rate as f32 * 0.012)).min(1.0);
        let decay = (-5.2 * t).exp();
        let fundamental = (frequency * t * std::f32::consts::TAU).sin();
        let overtone = (frequency * 2.01 * t * std::f32::consts::TAU).sin() * 0.24;
        output.push((fundamental + overtone) * decay * attack * volume);
    }
}

fn dual_tone(output: &mut Vec<f32>, rate: u32, a: f32, b: f32, seconds: f32, volume: f32) {
    let frames = (rate as f32 * seconds) as usize;
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let edge = ((frames - i) as f32 / (rate as f32 * 0.035)).min(1.0);
        let attack = (i as f32 / (rate as f32 * 0.025)).min(1.0);
        let envelope = edge.min(attack);
        let tone = (a * t * std::f32::consts::TAU).sin()
            + (b * t * std::f32::consts::TAU).sin() * 0.82;
        output.push(tone * envelope * volume);
    }
}

fn silence(output: &mut Vec<f32>, rate: u32, seconds: f32) {
    output.resize(output.len() + (rate as f32 * seconds) as usize, 0.0);
}

//! Efeitos locais curtos para chamadas em DM; não passam pela rede nem pelo mixer da voz.
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};
use std::{thread, time::Duration};

#[derive(Clone, Copy)]
pub enum Effect {
    Incoming,
    Outgoing,
    Connected,
    Ended,
}

pub fn play(output_name: Option<String>, effect: Effect) {
    let _ = thread::Builder::new()
        .name("telacast-call-sound".into())
        .spawn(move || play_blocking(output_name.as_deref(), effect));
}

fn play_blocking(output_name: Option<&str>, effect: Effect) {
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
        SampleFormat::F32 => stream::<f32>(&device, &config, samples.clone()),
        SampleFormat::F64 => stream::<f64>(&device, &config, samples.clone()),
        SampleFormat::I8 => stream::<i8>(&device, &config, samples.clone()),
        SampleFormat::I16 => stream::<i16>(&device, &config, samples.clone()),
        SampleFormat::I24 => stream::<cpal::I24>(&device, &config, samples.clone()),
        SampleFormat::I32 => stream::<i32>(&device, &config, samples.clone()),
        SampleFormat::I64 => stream::<i64>(&device, &config, samples.clone()),
        SampleFormat::U8 => stream::<u8>(&device, &config, samples.clone()),
        SampleFormat::U16 => stream::<u16>(&device, &config, samples.clone()),
        SampleFormat::U32 => stream::<u32>(&device, &config, samples.clone()),
        SampleFormat::U64 => stream::<u64>(&device, &config, samples.clone()),
        _ => return,
    };
    let Ok(stream) = stream else { return };
    if stream.play().is_ok() {
        thread::sleep(Duration::from_secs_f64(samples.len() as f64 / rate as f64) + Duration::from_millis(80));
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Vec<f32>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels.max(1) as usize;
    let mut frame = 0usize;
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            for channel_frame in output.chunks_mut(channels) {
                let value = samples.get(frame).copied().unwrap_or(0.0);
                for sample in channel_frame {
                    *sample = T::from_sample(value);
                }
                frame = frame.saturating_add(1);
            }
        },
        |_error| {},
        None,
    )
}

fn samples(effect: Effect, rate: u32) -> Vec<f32> {
    let notes: &[([f32; 2], u32, u32)] = match effect {
        Effect::Incoming => &[
            ([660.0, 880.0], 170, 70),
            ([660.0, 880.0], 170, 70),
            ([660.0, 880.0], 260, 180),
        ],
        Effect::Outgoing => &[
            ([440.0, 480.0], 280, 220),
            ([440.0, 480.0], 280, 0),
        ],
        Effect::Connected => &[
            ([523.25, 0.0], 85, 25),
            ([659.25, 0.0], 85, 25),
            ([783.99, 0.0], 150, 0),
        ],
        Effect::Ended => &[
            ([783.99, 0.0], 120, 35),
            ([659.25, 0.0], 120, 35),
            ([523.25, 0.0], 180, 0),
        ],
    };
    let mut result = Vec::new();
    for &([a, b], milliseconds, silence_ms) in notes {
        let frames = (rate as u64 * milliseconds as u64 / 1000) as usize;
        for i in 0..frames {
            let t = i as f32 / rate as f32;
            let fade_in = (i as f32 / (rate as f32 * 0.008)).min(1.0);
            let fade_out = ((frames - i) as f32 / (rate as f32 * 0.018)).min(1.0);
            let envelope = fade_in.min(fade_out);
            let tone = (a * t * std::f32::consts::TAU).sin() + if b > 0.0 { (b * t * std::f32::consts::TAU).sin() } else { 0.0 };
            result.push((tone * 0.055 * envelope).clamp(-0.12, 0.12));
        }
        result.resize(result.len() + rate as usize * silence_ms as usize / 1000, 0.0);
    }
    result
}

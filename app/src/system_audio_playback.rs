//! Reproduz a faixa de áudio do stream independentemente da voz.
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, FromSample, Sample, SampleFormat, SizedSample};
use opus::{Channels, Decoder};
use rtrb::{Consumer, Producer, RingBuffer};
use std::{sync::{atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering}, Arc}, time::Instant};

const RATE: u32 = 48_000;
const QUEUE_FRAMES: usize = 12_000; // 250 ms de PCM estéreo
const PREBUFFER_FRAMES: usize = 2_400; // 50 ms antes do primeiro som
const VIDEO_AUDIO_DELAY_US: u64 = 60_000;

pub struct Player {
    decoder: Decoder,
    decoded: Vec<f32>,
    prod: Producer<[f32; 2]>,
    muted: Arc<AtomicBool>,
    play_after_us: Arc<AtomicU64>,
    base_pts: u64,
    scheduled: bool,
    _stream: cpal::Stream,
    last_pts: Option<u64>,
}

impl Player {
    pub fn new(muted: Arc<AtomicBool>, volume: Arc<AtomicU32>, anchor: Instant, base_pts: u64) -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("nenhuma saída de áudio disponível")?;
        let supported = device.default_output_config().map_err(|e| format!("saída de áudio: {e}"))?;
        let config = supported.config();
        let channels = config.channels.max(1) as usize;
        let rate = config.sample_rate.0;
        let (prod, cons) = RingBuffer::<[f32; 2]>::new(QUEUE_FRAMES);
        let play_after_us = Arc::new(AtomicU64::new(u64::MAX));
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::F64 => build_stream::<f64>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::I8 => build_stream::<i8>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::I24 => build_stream::<cpal::I24>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::I64 => build_stream::<i64>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::U8 => build_stream::<u8>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::U32 => build_stream::<u32>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            SampleFormat::U64 => build_stream::<u64>(&device, &config, cons, muted.clone(), volume.clone(), play_after_us.clone(), anchor, channels, rate),
            other => return Err(format!("formato de saída não suportado: {other:?}")),
        }?;
        stream.play().map_err(|e| format!("início da saída de áudio: {e}"))?;
        Ok(Self {
            decoder: Decoder::new(RATE, Channels::Stereo).map_err(|e| format!("decoder Opus: {e}"))?,
            decoded: vec![0.0; 5_760 * 2],
            prod,
            muted,
            play_after_us,
            base_pts,
            scheduled: false,
            _stream: stream,
            last_pts: None,
        })
    }

    pub fn push(&mut self, pts_us: u64, packet: &[u8]) -> Result<(), String> {
        if self.last_pts.is_some_and(|last| pts_us < last) {
            return Ok(());
        }
        self.last_pts = Some(pts_us);
        if !self.scheduled {
            let due = pts_us.saturating_sub(self.base_pts).saturating_add(VIDEO_AUDIO_DELAY_US);
            self.play_after_us.store(due, Ordering::Release);
            self.scheduled = true;
        }
        let frames = self.decoder.decode_float(packet, &mut self.decoded, false).map_err(|e| format!("decode Opus: {e}"))?;
        for frame in self.decoded[..frames * 2].chunks_exact(2) {
            if self.prod.push([frame[0], frame[1]]).is_err() {
                // Fila cheia: descarta a amostra nova; a fila nunca acumula latência sem limite.
                break;
            }
        }
        Ok(())
    }

    pub fn mute(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }
}

struct Resampler {
    consumer: Consumer<[f32; 2]>,
    muted: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    play_after_us: Arc<AtomicU64>,
    anchor: Instant,
    channels: usize,
    step: f32,
    phase: f32,
    current: [f32; 2],
    next: [f32; 2],
    primed: bool,
    started: bool,
}

impl Resampler {
    fn source_frame(&mut self) -> [f32; 2] { self.consumer.pop().unwrap_or([0.0, 0.0]) }

    fn render<T: Sample + FromSample<f32>>(&mut self, out: &mut [T]) {
        if self.muted.load(Ordering::Relaxed) {
            while self.consumer.pop().is_ok() {}
            self.primed = false;
            self.phase = 0.0;
            for sample in out { *sample = T::from_sample(0.0); }
            return;
        }
        if !self.started {
            let due = self.play_after_us.load(Ordering::Acquire);
            if due == u64::MAX || self.anchor.elapsed().as_micros() < due as u128 {
                for sample in out { *sample = T::from_sample(0.0); }
                return;
            }
            if self.consumer.slots() < PREBUFFER_FRAMES {
                for sample in out { *sample = T::from_sample(0.0); }
                return;
            }
            self.started = true;
        }
        for frame in out.chunks_mut(self.channels) {
            if !self.primed {
                self.current = self.source_frame();
                self.next = self.source_frame();
                self.primed = true;
            }
            while self.phase >= 1.0 {
                self.current = self.next;
                self.next = self.source_frame();
                self.phase -= 1.0;
            }
            let left = self.current[0] + (self.next[0] - self.current[0]) * self.phase;
            let right = self.current[1] + (self.next[1] - self.current[1]) * self.phase;
            let gain = self.volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
            let stereo = [(left * gain).clamp(-1.0, 1.0), (right * gain).clamp(-1.0, 1.0)];
            for (i, sample) in frame.iter_mut().enumerate() {
                let value = match i { 0 => stereo[0], 1 => stereo[1], _ => 0.0 };
                *sample = T::from_sample(value);
            }
            self.phase += self.step;
        }
    }
}

fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    consumer: Consumer<[f32; 2]>,
    muted: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    play_after_us: Arc<AtomicU64>,
    anchor: Instant,
    channels: usize,
    rate: u32,
) -> Result<cpal::Stream, String> {
    let error = |_e| {};
    let mut resampler = Resampler {
        consumer,
        muted,
        volume,
        play_after_us,
        anchor,
        channels,
        step: RATE as f32 / rate.max(1) as f32,
        phase: 0.0,
        current: [0.0; 2],
        next: [0.0; 2],
        primed: false,
        started: false,
    };
    device
        .build_output_stream(config, move |data: &mut [T], _| resampler.render(data), error, None)
        .map_err(|e| e.to_string())
}

//! Captura de telas e janelas com Windows Graphics Capture.
use crate::capture::Scaler;
use anyhow::{anyhow, Context as _};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::Duration,
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

#[derive(Clone, PartialEq, Debug)]
pub enum Source {
    Monitor(usize),
    Window(usize),
}

#[derive(Clone, PartialEq, Debug)]
pub struct SourceInfo {
    pub source: Source,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct Raw {
    pub w: u32,
    pub h: u32,
    pub data: Arc<Vec<u8>>,
}

type Captured = Result<Raw, String>;
type FrameSender = Sender<Captured>;

struct FrameHandler {
    flags: (FrameSender, Arc<AtomicBool>),
}

impl GraphicsCaptureApiHandler for FrameHandler {
    type Flags = (FrameSender, Arc<AtomicBool>);
    type Error = io::Error;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { flags: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // A fila tem um único quadro: descarte antes de copiar pixels quando o
        // consumidor ainda está ocupado codificando o quadro anterior.
        if self
            .flags
            .1
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        let (w, h) = (frame.width(), frame.height());
        let mut packed = Vec::new();
        let buffer = match frame.buffer() {
            Ok(buffer) => buffer,
            Err(e) => {
                self.flags.1.store(false, Ordering::Relaxed);
                return Err(io::Error::other(e));
            }
        };
        if buffer.has_padding() {
            let _ = buffer.as_nopadding_buffer(&mut packed);
        } else {
            let pixels = buffer.as_nopadding_buffer(&mut packed).to_vec();
            packed = pixels;
        }
        if self.flags.0.send(Ok(Raw { w, h, data: Arc::new(packed) })).is_err() {
            self.flags.1.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        let _ = self
            .flags
            .0
            .send(Err("a tela ou janela encerrou a captura".to_owned()));
        Ok(())
    }
}

pub fn list_monitors() -> Vec<SourceInfo> {
    Monitor::enumerate()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, monitor)| {
            let width = monitor.width().ok()?;
            let height = monitor.height().ok()?;
            (width > 0 && height > 0).then(|| SourceInfo {
                source: Source::Monitor(index),
                title: monitor
                    .device_name()
                    .unwrap_or_else(|_| format!("Tela {}", index + 1)),
                width,
                height,
            })
        })
        .collect()
}

pub fn list_windows() -> Vec<SourceInfo> {
    Window::enumerate()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|window| {
            let title = window.title().ok()?;
            if title.trim().is_empty() || title.trim() == "Telacast" {
                return None;
            }
            let width = window.width().ok()? as u32;
            let height = window.height().ok()? as u32;
            if width < 64 || height < 64 {
                return None;
            }
            Some(SourceInfo {
                source: Source::Window(window.as_raw_hwnd() as usize),
                title,
                width,
                height,
            })
        })
        .collect()
}

fn start_capture<T>(
    item: T,
) -> anyhow::Result<(Receiver<Captured>, CaptureControl<FrameHandler, io::Error>)>
where
    T: TryInto<windows_capture::settings::GraphicsCaptureItemType> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let has_pending_frame = Arc::new(AtomicBool::new(false));
    let settings = Settings::new(
        item,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (tx, has_pending_frame),
    );
    let control = FrameHandler::start_free_threaded(settings)
        .map_err(|e| anyhow!("não foi possível iniciar a captura Windows: {e}"))?;
    Ok((rx, control))
}

pub struct Grabber {
    rx: Receiver<Captured>,
    has_pending_frame: Arc<AtomicBool>,
    control: Option<CaptureControl<FrameHandler, io::Error>>,
    width: u32,
    height: u32,
    is_window: bool,
    last_frame: Mutex<Option<Raw>>,
}

impl Grabber {
    pub fn new(source: &Source) -> anyhow::Result<Self> {
        let (rx, control, width, height) = match source {
            Source::Monitor(index) => {
                let monitor = Monitor::enumerate()
                    .context("não consegui enumerar as telas")?
                    .into_iter()
                    .filter(|m| m.width().is_ok_and(|w| w > 0) && m.height().is_ok_and(|h| h > 0))
                    .nth(*index)
                    .ok_or_else(|| anyhow!("tela não encontrada"))?;
                let width = monitor.width()?;
                let height = monitor.height()?;
                let (rx, control) = start_capture(monitor)?;
                (rx, control, width, height)
            }
            Source::Window(id) => {
                let window = Window::enumerate()
                    .context("não consegui enumerar as janelas")?
                    .into_iter()
                    .find(|w| w.as_raw_hwnd() as usize == *id)
                    .ok_or_else(|| anyhow!("a janela selecionada não está mais disponível"))?;
                let width = window.width()?.max(0) as u32;
                let height = window.height()?.max(0) as u32;
                let (rx, control) = start_capture(window)?;
                (rx, control, width, height)
            }
        };
        let has_pending_frame = control.callback().lock().flags.1.clone();
        Ok(Self {
            rx,
            has_pending_frame,
            control: Some(control),
            width,
            height,
            is_window: matches!(source, Source::Window(_)),
            last_frame: Mutex::new(None),
        })
    }

    pub fn is_window(&self) -> bool {
        self.is_window
    }

    pub fn size(&self) -> anyhow::Result<(u32, u32)> {
        Ok((self.width, self.height))
    }

    pub fn grab(&self) -> anyhow::Result<Raw> {
        let cached = self.last_frame.lock().unwrap().clone();
        let result = if let Some(cached) = cached {
            match self.rx.recv_timeout(Duration::from_millis(1)) {
                Ok(frame) => frame,
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(cached),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("a captura Windows foi encerrada"));
                }
            }
        } else {
            self.rx.recv().context("a captura Windows foi encerrada")?
        };
        self.has_pending_frame.store(false, Ordering::Relaxed);
        let raw = result.map_err(|e| anyhow!(e))?;
        *self.last_frame.lock().unwrap() = Some(raw.clone());
        Ok(raw)
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| control.stop()));
        }
    }
}

pub fn thumbnail(source: &Source, max_w: u32) -> Option<Thumb> {
    let raw = Grabber::new(source).ok()?.grab().ok()?;
    if raw.w == 0 || raw.h == 0 {
        return None;
    }
    let dw = raw.w.min(max_w.max(1));
    let dh = ((raw.h as u64 * dw as u64 + raw.w as u64 / 2) / raw.w as u64).max(1) as u32;
    let mut rgba = vec![0; dw as usize * dh as usize * 4];
    Scaler::default().scale(
        &raw.data,
        raw.w as usize,
        raw.w as usize,
        raw.h as usize,
        &mut rgba,
        dw as usize,
        dw as usize,
        dh as usize,
    );
    for p in rgba.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    Some(Thumb {
        width: dw,
        height: dh,
        rgba,
    })
}

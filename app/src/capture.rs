#[path = "capture_scaler.rs"]
mod scaler;
pub use scaler::Scaler;

#[cfg(windows)]
#[path = "capture_windows.rs"]
mod backend;
#[cfg(not(windows))]
#[path = "capture_x11.rs"]
mod backend;

pub use backend::{
    list_monitors, list_windows, thumbnail, Grabber, Raw, Source, SourceInfo, Thumb,
};

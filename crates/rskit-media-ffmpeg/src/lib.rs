//! FFmpeg CLI backend for video/audio processing.
//!
//! Implements [`MediaProbe`](rskit_media::MediaProbe) and
//! [`MediaExecutor`](rskit_media::MediaExecutor) by shelling out
//! to `ffprobe` and `ffmpeg` CLI tools.

#![warn(missing_docs)]

mod command;
mod config;
mod error;
mod executor;
mod filter_map;
mod hw_accel;
mod probe;
mod progress;

pub use command::FfmpegCommand;
pub use config::{FfmpegConfig, FfmpegLogLevel};
pub use error::{classify_error, FfmpegError, FfmpegErrorKind};
pub use executor::FfmpegExecutor;
pub use hw_accel::HwAccel;
pub use probe::FfmpegProbe;

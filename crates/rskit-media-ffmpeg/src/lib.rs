//! FFmpeg CLI backend for video/audio processing.
//!
//! Implements [`MediaProbe`](rskit_media::MediaProbe) and
//! [`MediaExecutor`](rskit_media::MediaExecutor) by shelling out
//! to `ffprobe` and `ffmpeg` CLI tools.

#![warn(missing_docs)]

mod config;
mod probe;
mod executor;
mod command;
mod filter_map;
mod progress;
mod hw_accel;

pub use config::{FfmpegConfig, FfmpegLogLevel};
pub use probe::FfmpegProbe;
pub use executor::FfmpegExecutor;
pub use hw_accel::HwAccel;

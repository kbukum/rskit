//! FFmpeg CLI backend for video/audio processing.
//!
//! Registers [`MediaProbe`](rskit_media::MediaProbe)
//! and [`MediaExecutor`](rskit_media::MediaExecutor) by shelling out to `ffprobe`
//! and `ffmpeg` CLI tools.

#![warn(missing_docs)]

mod command;
mod compilers;
mod config;
mod error;
mod executor;
mod filter_map;
mod hw_accel;
mod paths;
mod probe;
mod process;
mod progress;
mod registration;
#[cfg(all(test, unix))]
mod test_support;

pub use config::FfmpegConfig as Config;
pub use registration::register;

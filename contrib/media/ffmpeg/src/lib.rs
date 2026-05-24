//! FFmpeg CLI backend for video/audio processing.
//!
//! Registers [`MediaProbe`](rskit_media::MediaProbe) and
//! [`MediaExecutor`](rskit_media::MediaExecutor) by shelling out
//! to `ffprobe` and `ffmpeg` CLI tools.

#![warn(missing_docs)]

mod command;
mod compilers;
mod config;
mod error;
mod executor;
mod filter_map;
mod hw_accel;
mod probe;
mod process;
mod progress;

use std::sync::Arc;

pub use config::FfmpegConfig as Config;

#[doc(hidden)]
pub mod __private {
    pub use crate::command::{FfmpegCommand, SourceHints};
    pub use crate::error::{FfmpegError, FfmpegErrorKind, classify_error};
    pub use crate::executor::FfmpegExecutor;
    pub use crate::probe::FfmpegProbe;
}

/// Register configured FFmpeg media executor and probe factories.
pub fn register(
    registry: &mut rskit_media::Registry,
    config: Config,
) -> rskit_errors::AppResult<()> {
    let executor_config = config.clone();
    registry.register_executor(
        "ffmpeg",
        Arc::new(move || {
            Ok(Arc::new(executor::FfmpegExecutor::new(
                executor_config.clone(),
                rskit_media::Registry::default(),
            )) as Arc<dyn rskit_media::MediaExecutor>)
        }),
    )?;
    registry.register_probe(
        "ffmpeg",
        Arc::new(move || {
            Ok(Arc::new(probe::FfmpegProbe::new(config.clone()))
                as Arc<dyn rskit_media::MediaProbe>)
        }),
    )
}

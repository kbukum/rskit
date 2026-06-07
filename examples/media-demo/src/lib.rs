//! Shared helpers for media demo binaries.

use std::path::PathBuf;

use rskit::media_ffmpeg::Config as FfmpegConfig;
use rskit::{AppError, AppResult, ErrorCode};

/// Build FFmpeg demo configuration with local paths confined to the current directory.
///
/// The examples accept CLI file paths and pass them to FFmpeg subprocesses. Confining
/// those paths to the invocation directory demonstrates the secure-by-default adapter
/// configuration while still keeping the examples easy to run from a media workspace.
pub fn ffmpeg_config() -> AppResult<FfmpegConfig> {
    Ok(FfmpegConfig::default().with_path_root(current_dir()?))
}

fn current_dir() -> AppResult<PathBuf> {
    std::env::current_dir()
        .map_err(|error| AppError::new(ErrorCode::Internal, format!("failed to read cwd: {error}")))
}

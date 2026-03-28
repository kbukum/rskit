//! FFmpeg configuration types.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::hw_accel::HwAccel;

/// Configuration for the FFmpeg backend.
#[derive(Debug, Clone, Deserialize)]
pub struct FfmpegConfig {
    /// Path to the `ffmpeg` binary (auto-detected if `None`).
    pub ffmpeg_path: Option<PathBuf>,
    /// Path to the `ffprobe` binary (auto-detected if `None`).
    pub ffprobe_path: Option<PathBuf>,
    /// Directory for temporary files.
    pub temp_dir: Option<PathBuf>,
    /// Number of threads to use.
    pub threads: Option<u32>,
    /// Hardware acceleration mode.
    pub hw_accel: Option<HwAccel>,
    /// Maximum execution timeout.
    pub timeout: Option<Duration>,
    /// Whether to overwrite existing output files (`-y` flag).
    pub overwrite: bool,
    /// FFmpeg log level.
    pub log_level: FfmpegLogLevel,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: None,
            ffprobe_path: None,
            temp_dir: None,
            threads: None,
            hw_accel: None,
            timeout: None,
            overwrite: true,
            log_level: FfmpegLogLevel::Warning,
        }
    }
}

impl FfmpegConfig {
    /// Resolve the path to the `ffmpeg` binary.
    pub fn ffmpeg_bin(&self) -> PathBuf {
        self.ffmpeg_path
            .clone()
            .unwrap_or_else(|| which::which("ffmpeg").unwrap_or_else(|_| PathBuf::from("ffmpeg")))
    }

    /// Resolve the path to the `ffprobe` binary.
    pub fn ffprobe_bin(&self) -> PathBuf {
        self.ffprobe_path
            .clone()
            .unwrap_or_else(|| which::which("ffprobe").unwrap_or_else(|_| PathBuf::from("ffprobe")))
    }
}

/// FFmpeg log verbosity level.
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum FfmpegLogLevel {
    /// Suppress all output.
    Quiet,
    /// Only show critical errors.
    Panic,
    /// Show fatal errors.
    Fatal,
    /// Show errors.
    Error,
    /// Show warnings and errors.
    Warning,
    /// Show informational messages.
    Info,
    /// Show verbose output.
    Verbose,
    /// Show debug output.
    Debug,
}

impl FfmpegLogLevel {
    /// Convert to the FFmpeg `-loglevel` argument value.
    pub fn as_ffmpeg_arg(&self) -> &str {
        match self {
            Self::Quiet => "quiet",
            Self::Panic => "panic",
            Self::Fatal => "fatal",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Verbose => "verbose",
            Self::Debug => "debug",
        }
    }
}

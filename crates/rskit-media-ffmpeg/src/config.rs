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
    /// Number of threads to use per FFmpeg process.
    pub threads: Option<u32>,
    /// Hardware acceleration mode.
    pub hw_accel: Option<HwAccel>,
    /// Maximum execution timeout per FFmpeg invocation.
    pub timeout: Option<Duration>,
    /// Whether to overwrite existing output files (`-y` flag).
    pub overwrite: bool,
    /// FFmpeg log level.
    pub log_level: FfmpegLogLevel,
    /// Maximum number of concurrent FFmpeg processes.
    /// Defaults to `num_cpus / 2` (minimum 1) if `None`.
    pub max_concurrent: Option<usize>,
    /// When `true`, if an FFmpeg invocation fails due to hardware acceleration
    /// issues (e.g., macOS exit code 69 / VideoToolbox exhaustion), automatically
    /// retry with software-only decoding (`-hwaccel none`).
    /// Defaults to `true`.
    #[serde(default = "default_hw_accel_fallback")]
    pub hw_accel_fallback: bool,
    /// Maximum number of retries for transient failures (hw accel exhaustion,
    /// timeouts). Does not retry permanent failures (invalid input, bad codec).
    /// Defaults to `1`.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Maximum number of stderr lines to include in error messages.
    /// Defaults to `100`.
    #[serde(default = "default_max_stderr_lines")]
    pub max_stderr_lines: usize,
    /// Override the input video decoder (e.g., `"libdav1d"` for software AV1 decode).
    /// When set, emits `-c:v <decoder>` before the input, forcing FFmpeg to use
    /// this specific decoder instead of the default one.
    #[serde(default)]
    pub input_video_decoder: Option<String>,
}

fn default_hw_accel_fallback() -> bool {
    true
}

fn default_max_retries() -> u32 {
    1
}

fn default_max_stderr_lines() -> usize {
    100
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
            max_concurrent: None,
            hw_accel_fallback: true,
            max_retries: 1,
            max_stderr_lines: 100,
            input_video_decoder: None,
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

    /// Effective max concurrent processes.
    pub fn effective_max_concurrent(&self) -> usize {
        self.max_concurrent
            .unwrap_or_else(|| {
                let cpus = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4);
                (cpus / 2).max(1)
            })
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

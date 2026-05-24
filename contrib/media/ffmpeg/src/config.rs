//! FFmpeg configuration types.

use std::path::PathBuf;
use std::time::Duration;

use rskit_media::timeout::{OperationKind, TimeoutCalculator};
use serde::Deserialize;

use crate::hw_accel::HwAccel;

/// Configuration for the FFmpeg backend.
#[derive(Debug, Clone, Deserialize)]
pub struct FfmpegConfig {
    /// Path to the `ffmpeg` binary (auto-detected if `None`).
    pub(crate) ffmpeg_path: Option<PathBuf>,
    /// Path to the `ffprobe` binary (auto-detected if `None`).
    pub(crate) ffprobe_path: Option<PathBuf>,
    /// Directory for temporary files.
    pub(crate) temp_dir: Option<PathBuf>,
    /// Number of threads to use per FFmpeg process.
    pub(crate) threads: Option<u32>,
    /// Hardware acceleration mode.
    pub(crate) hw_accel: Option<HwAccel>,
    /// Fixed execution timeout per FFmpeg invocation.
    ///
    /// When a [`TimeoutCalculator`] is also configured, the calculator takes
    /// precedence if source duration and operation kind are available. This
    /// fixed timeout serves as the fallback when duration is unknown.
    pub(crate) timeout: Option<Duration>,
    /// Duration-aware timeout calculator.
    ///
    /// When set, timeouts are computed dynamically based on source duration
    /// and operation type using `base + (duration × multiplier)`. Falls back
    /// to the fixed `timeout` field when source duration is not available.
    #[serde(skip)]
    pub(crate) timeout_calculator: Option<TimeoutCalculator>,
    /// Whether to overwrite existing output files (`-y` flag).
    pub(crate) overwrite: bool,
    /// FFmpeg log level.
    pub(crate) log_level: FfmpegLogLevel,
    /// Maximum number of concurrent FFmpeg processes.
    /// Defaults to `num_cpus / 2` (minimum 1) if `None`.
    pub(crate) max_concurrent: Option<usize>,
    /// When `true`, if an FFmpeg invocation fails due to hardware acceleration
    /// issues (e.g., macOS exit code 69 / VideoToolbox exhaustion), automatically
    /// retry with software-only decoding (`-hwaccel none`).
    /// Defaults to `true`.
    #[serde(default = "default_hw_accel_fallback")]
    pub(crate) hw_accel_fallback: bool,
    /// Maximum number of retries for transient failures (hw accel exhaustion,
    /// timeouts). Does not retry permanent failures (invalid input, bad codec).
    /// Defaults to `1`.
    #[serde(default = "default_max_retries")]
    pub(crate) max_retries: u32,
    /// Maximum number of stderr lines to include in error messages.
    /// Defaults to `100`.
    #[serde(default = "default_max_stderr_lines")]
    pub(crate) max_stderr_lines: usize,
    /// Override the input video decoder (e.g., `"libdav1d"` for software AV1 decode).
    /// When set, emits `-c:v <decoder>` before the input, forcing FFmpeg to use
    /// this specific decoder instead of the default one.
    #[serde(default)]
    pub(crate) input_video_decoder: Option<String>,
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
            timeout_calculator: None,
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
    /// Override the path to the `ffmpeg` binary.
    #[must_use]
    pub fn with_ffmpeg_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.ffmpeg_path = Some(path.into());
        self
    }

    /// Override the path to the `ffprobe` binary.
    #[must_use]
    pub fn with_ffprobe_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.ffprobe_path = Some(path.into());
        self
    }

    /// Override the directory used for temporary files.
    #[must_use]
    pub fn with_temp_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.temp_dir = Some(path.into());
        self
    }

    /// Override the FFmpeg thread count.
    #[must_use]
    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = Some(threads);
        self
    }

    /// Force software-only decoding.
    #[must_use]
    pub fn with_software_decode(mut self) -> Self {
        self.hw_accel = Some(HwAccel::None);
        self
    }

    /// Let FFmpeg auto-detect hardware acceleration.
    #[must_use]
    pub fn with_auto_hw_accel(mut self) -> Self {
        self.hw_accel = Some(HwAccel::Auto);
        self
    }

    /// Prefer macOS VideoToolbox hardware acceleration.
    #[must_use]
    pub fn with_videotoolbox(mut self) -> Self {
        self.hw_accel = Some(HwAccel::VideoToolbox);
        self
    }

    /// Prefer NVIDIA CUDA hardware acceleration.
    #[must_use]
    pub fn with_cuda(mut self) -> Self {
        self.hw_accel = Some(HwAccel::Cuda);
        self
    }

    /// Prefer Intel Quick Sync Video hardware acceleration.
    #[must_use]
    pub fn with_qsv(mut self) -> Self {
        self.hw_accel = Some(HwAccel::Qsv);
        self
    }

    /// Prefer VA-API hardware acceleration.
    #[must_use]
    pub fn with_vaapi(mut self) -> Self {
        self.hw_accel = Some(HwAccel::Vaapi);
        self
    }

    /// Prefer Vulkan hardware acceleration.
    #[must_use]
    pub fn with_vulkan(mut self) -> Self {
        self.hw_accel = Some(HwAccel::Vulkan);
        self
    }

    /// Prefer Direct3D 11 Video Acceleration.
    #[must_use]
    pub fn with_d3d11va(mut self) -> Self {
        self.hw_accel = Some(HwAccel::D3d11va);
        self
    }

    /// Override the fixed execution timeout used when no duration-aware timeout applies.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set whether FFmpeg should overwrite existing output files.
    #[must_use]
    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Use quiet FFmpeg logging.
    #[must_use]
    pub fn with_quiet_log_level(mut self) -> Self {
        self.log_level = FfmpegLogLevel::Quiet;
        self
    }

    /// Use error-only FFmpeg logging.
    #[must_use]
    pub fn with_error_log_level(mut self) -> Self {
        self.log_level = FfmpegLogLevel::Error;
        self
    }

    /// Use warning-level FFmpeg logging.
    #[must_use]
    pub fn with_warning_log_level(mut self) -> Self {
        self.log_level = FfmpegLogLevel::Warning;
        self
    }

    /// Use informational FFmpeg logging.
    #[must_use]
    pub fn with_info_log_level(mut self) -> Self {
        self.log_level = FfmpegLogLevel::Info;
        self
    }

    /// Use debug FFmpeg logging.
    #[must_use]
    pub fn with_debug_log_level(mut self) -> Self {
        self.log_level = FfmpegLogLevel::Debug;
        self
    }

    /// Override the maximum number of concurrent FFmpeg processes.
    #[must_use]
    pub fn with_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }

    /// Set whether hardware acceleration errors should fall back to software decoding.
    #[must_use]
    pub fn with_hw_accel_fallback(mut self, enabled: bool) -> Self {
        self.hw_accel_fallback = enabled;
        self
    }

    /// Override the maximum retry count for transient FFmpeg failures.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Override the maximum stderr lines included in error messages.
    #[must_use]
    pub fn with_max_stderr_lines(mut self, max_stderr_lines: usize) -> Self {
        self.max_stderr_lines = max_stderr_lines;
        self
    }

    /// Override the input video decoder.
    #[must_use]
    pub fn with_input_video_decoder(mut self, decoder: impl Into<String>) -> Self {
        self.input_video_decoder = Some(decoder.into());
        self
    }

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
        self.max_concurrent.unwrap_or_else(|| {
            let cpus = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            (cpus / 2).max(1)
        })
    }

    /// Resolve the effective timeout for an operation.
    ///
    /// Priority:
    /// 1. If a [`TimeoutCalculator`] is configured and `source_duration` is
    ///    provided, compute a duration-aware timeout based on the operation kind.
    /// 2. Otherwise, fall back to the fixed `timeout` field.
    /// 3. If neither is set, returns `None` (no timeout).
    #[must_use]
    pub fn resolve_timeout(
        &self,
        source_duration: Option<Duration>,
        op_kind: Option<OperationKind>,
    ) -> Option<Duration> {
        if let (Some(calc), Some(dur)) = (&self.timeout_calculator, source_duration) {
            let kind = op_kind.unwrap_or(OperationKind::Transcode);
            Some(calc.calculate(dur, kind))
        } else {
            self.timeout
        }
    }

    /// Set the timeout calculator (builder-style).
    #[must_use]
    pub fn with_timeout_calculator(mut self, calc: TimeoutCalculator) -> Self {
        self.timeout_calculator = Some(calc);
        self
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

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
    /// Optional existing root for user-provided local media paths.
    ///
    /// When configured, local `FileSource::Path` inputs and `FileSink::Path`
    /// outputs must resolve under this root after canonicalization. Temporary
    /// files created by the adapter are not confined by this setting.
    pub(crate) path_root: Option<PathBuf>,
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
            path_root: None,
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

    /// Confine user-provided local input and output paths to an existing root.
    ///
    /// Relative media paths are resolved under this root. Absolute media paths
    /// are accepted only when they canonicalize under the root. Output paths may
    /// be missing, but their nearest existing ancestor must stay under the root.
    #[must_use]
    pub fn with_path_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.path_root = Some(path.into());
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

    /// Return the optional root used to confine user-provided local media paths.
    #[must_use]
    pub fn path_root(&self) -> Option<&std::path::Path> {
        self.path_root.as_deref()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_methods_set_all_runtime_knobs() {
        let config = FfmpegConfig::default()
            .with_ffmpeg_path("/opt/bin/ffmpeg")
            .with_ffprobe_path("/opt/bin/ffprobe")
            .with_temp_dir("/tmp/rskit-ffmpeg")
            .with_path_root("/media")
            .with_threads(4)
            .with_software_decode()
            .with_auto_hw_accel()
            .with_videotoolbox()
            .with_cuda()
            .with_qsv()
            .with_vaapi()
            .with_vulkan()
            .with_d3d11va()
            .with_timeout(Duration::from_secs(30))
            .with_overwrite(false)
            .with_quiet_log_level()
            .with_error_log_level()
            .with_warning_log_level()
            .with_info_log_level()
            .with_debug_log_level()
            .with_max_concurrent(3)
            .with_hw_accel_fallback(false)
            .with_max_retries(2)
            .with_max_stderr_lines(7)
            .with_input_video_decoder("libdav1d");

        assert_eq!(config.ffmpeg_bin(), PathBuf::from("/opt/bin/ffmpeg"));
        assert_eq!(config.ffprobe_bin(), PathBuf::from("/opt/bin/ffprobe"));
        assert_eq!(
            config.temp_dir.as_deref(),
            Some(std::path::Path::new("/tmp/rskit-ffmpeg"))
        );
        assert_eq!(config.path_root(), Some(std::path::Path::new("/media")));
        assert_eq!(config.threads, Some(4));
        assert!(matches!(config.hw_accel, Some(HwAccel::D3d11va)));
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert!(!config.overwrite);
        assert!(matches!(config.log_level, FfmpegLogLevel::Debug));
        assert_eq!(config.effective_max_concurrent(), 3);
        assert!(!config.hw_accel_fallback);
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.max_stderr_lines, 7);
        assert_eq!(config.input_video_decoder.as_deref(), Some("libdav1d"));
    }

    #[test]
    fn default_binary_resolution_falls_back_to_binary_names() {
        let config = FfmpegConfig::default();

        let ffmpeg = config.ffmpeg_bin();
        let ffprobe = config.ffprobe_bin();

        assert!(ffmpeg.ends_with("ffmpeg"));
        assert!(ffprobe.ends_with("ffprobe"));
    }

    #[test]
    fn max_concurrent_defaults_to_at_least_one() {
        assert!(FfmpegConfig::default().effective_max_concurrent() >= 1);
    }

    #[test]
    fn timeout_calculator_takes_precedence_when_duration_is_known() {
        let calculator = TimeoutCalculator::default()
            .with_base_timeout(Duration::from_secs(10))
            .with_max_timeout(Duration::from_secs(1_000))
            .with_multiplier(OperationKind::Filter, 1.0);
        let config = FfmpegConfig::default()
            .with_timeout(Duration::from_secs(5))
            .with_timeout_calculator(calculator);

        let timeout =
            config.resolve_timeout(Some(Duration::from_secs(20)), Some(OperationKind::Filter));

        assert_eq!(timeout, Some(Duration::from_secs(140)));
    }

    #[test]
    fn timeout_resolution_falls_back_to_fixed_timeout() {
        let calculator = TimeoutCalculator::default();
        let config = FfmpegConfig::default()
            .with_timeout(Duration::from_secs(5))
            .with_timeout_calculator(calculator);

        assert_eq!(
            config.resolve_timeout(None, Some(OperationKind::Filter)),
            Some(Duration::from_secs(5))
        );
        assert_eq!(FfmpegConfig::default().resolve_timeout(None, None), None);
    }

    #[test]
    fn log_levels_map_to_ffmpeg_arguments() {
        let cases = [
            (FfmpegLogLevel::Quiet, "quiet"),
            (FfmpegLogLevel::Panic, "panic"),
            (FfmpegLogLevel::Fatal, "fatal"),
            (FfmpegLogLevel::Error, "error"),
            (FfmpegLogLevel::Warning, "warning"),
            (FfmpegLogLevel::Info, "info"),
            (FfmpegLogLevel::Verbose, "verbose"),
            (FfmpegLogLevel::Debug, "debug"),
        ];

        for (level, expected) in cases {
            assert_eq!(level.as_ffmpeg_arg(), expected);
        }
    }
}

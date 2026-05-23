//! FFmpeg error classification — parses exit codes and stderr to categorize failures.

use rskit_errors::{AppError, ErrorCode};

/// Classified FFmpeg error kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegErrorKind {
    /// Hardware acceleration unavailable (macOS exit 69, CUDA/VAAPI OOM, etc.).
    /// Retryable with software fallback.
    HwAccelUnavailable,
    /// FFmpeg process timed out. Retryable.
    Timeout,
    /// Process was cancelled by user/system. Not retryable.
    Cancelled,
    /// Invalid input file (corrupt, unsupported format). Not retryable.
    InvalidInput,
    /// Encoder/decoder error (codec missing, license issue). Not retryable.
    CodecError,
    /// Output write error (disk full, permission denied). Not retryable.
    OutputError,
    /// FFmpeg binary not found or failed to spawn. Not retryable.
    SpawnFailed,
    /// Unclassified failure. May be retryable depending on context.
    Unknown,
}

impl FfmpegErrorKind {
    /// Whether this error kind is retryable (possibly with different config).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::HwAccelUnavailable | Self::Timeout | Self::Unknown
        )
    }

    /// Whether this error should trigger hw accel fallback specifically.
    pub fn should_fallback_hw_accel(&self) -> bool {
        matches!(self, Self::HwAccelUnavailable)
    }
}

/// Result of running an FFmpeg process that failed.
#[derive(Debug)]
pub struct FfmpegError {
    /// Classified error kind.
    pub kind: FfmpegErrorKind,
    /// Exit code from FFmpeg process, if available.
    pub exit_code: Option<i32>,
    /// Captured stderr output (last N lines).
    pub stderr: String,
    /// Human-readable error message.
    pub message: String,
}

impl FfmpegError {
    /// Convert to an `AppError` with appropriate error code.
    pub fn into_app_error(self) -> AppError {
        let code = match self.kind {
            FfmpegErrorKind::HwAccelUnavailable => ErrorCode::ServiceUnavailable,
            FfmpegErrorKind::Timeout => ErrorCode::Timeout,
            FfmpegErrorKind::Cancelled => ErrorCode::Cancelled,
            FfmpegErrorKind::InvalidInput => ErrorCode::InvalidInput,
            FfmpegErrorKind::CodecError => ErrorCode::Internal,
            FfmpegErrorKind::OutputError => ErrorCode::Internal,
            FfmpegErrorKind::SpawnFailed => ErrorCode::ServiceUnavailable,
            FfmpegErrorKind::Unknown => ErrorCode::Internal,
        };

        let detail = if self.stderr.is_empty() {
            self.message.clone()
        } else {
            format!("{}\nstderr:\n{}", self.message, self.stderr)
        };

        let mut err = AppError::new(code, detail);
        if self.kind.is_retryable() {
            err = err.retryable(true);
        }
        err
    }
}

/// Classify an FFmpeg failure from exit code and captured stderr.
pub fn classify_error(exit_code: Option<i32>, stderr: &str) -> FfmpegErrorKind {
    // Check exit code first
    if let Some(code) = exit_code {
        match code {
            // macOS EX_UNAVAILABLE — hardware resource exhaustion
            69 => return FfmpegErrorKind::HwAccelUnavailable,
            // Killed by signal (SIGKILL=137, SIGTERM=143)
            137 | 143 => return FfmpegErrorKind::Cancelled,
            _ => {}
        }
    }

    let stderr_lower = stderr.to_lowercase();

    // Hardware acceleration patterns
    if stderr_lower.contains("videotoolbox")
        || stderr_lower.contains("hwaccel")
        || stderr_lower.contains("hw_frames_ctx")
        || stderr_lower.contains("cuda")
        || stderr_lower.contains("failed to initialise")
        || stderr_lower.contains("no decoder surface")
        || stderr_lower.contains("hardware accelerat")
        || stderr_lower.contains("vt_decode")
    {
        return FfmpegErrorKind::HwAccelUnavailable;
    }

    // Invalid input patterns
    if stderr_lower.contains("invalid data found")
        || stderr_lower.contains("no such file")
        || stderr_lower.contains("does not exist")
        || stderr_lower.contains("not a valid")
        || stderr_lower.contains("invalid argument")
        || stderr_lower.contains("moov atom not found")
        || stderr_lower.contains("could not find codec")
    {
        return FfmpegErrorKind::InvalidInput;
    }

    // Codec/encoder errors
    if stderr_lower.contains("encoder") && stderr_lower.contains("not found")
        || stderr_lower.contains("decoder") && stderr_lower.contains("not found")
        || stderr_lower.contains("unknown encoder")
        || stderr_lower.contains("unknown decoder")
        || stderr_lower.contains("codec not currently supported")
    {
        return FfmpegErrorKind::CodecError;
    }

    // Output errors
    if stderr_lower.contains("no space left")
        || stderr_lower.contains("permission denied")
        || stderr_lower.contains("read-only file system")
        || stderr_lower.contains("is a directory")
    {
        return FfmpegErrorKind::OutputError;
    }

    FfmpegErrorKind::Unknown
}

/// Truncate stderr to a reasonable length for error messages.
/// Keeps the last `max_lines` lines which are most diagnostic.
pub fn truncate_stderr(stderr: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    if lines.len() <= max_lines {
        return stderr.to_string();
    }
    let start = lines.len() - max_lines;
    format!(
        "... ({} lines truncated) ...\n{}",
        start,
        lines[start..].join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_exit_69_as_hw_accel() {
        assert_eq!(
            classify_error(Some(69), ""),
            FfmpegErrorKind::HwAccelUnavailable
        );
    }

    #[test]
    fn classify_videotoolbox_stderr() {
        assert_eq!(
            classify_error(Some(1), "VideoToolbox session failed to create"),
            FfmpegErrorKind::HwAccelUnavailable
        );
    }

    #[test]
    fn classify_invalid_input() {
        assert_eq!(
            classify_error(Some(1), "Invalid data found when processing input"),
            FfmpegErrorKind::InvalidInput
        );
    }

    #[test]
    fn classify_codec_error() {
        assert_eq!(
            classify_error(Some(1), "Unknown encoder 'libx265_missing'"),
            FfmpegErrorKind::CodecError
        );
    }

    #[test]
    fn classify_output_error() {
        assert_eq!(
            classify_error(Some(1), "No space left on device"),
            FfmpegErrorKind::OutputError
        );
    }

    #[test]
    fn classify_signal_kill() {
        assert_eq!(classify_error(Some(137), ""), FfmpegErrorKind::Cancelled);
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(
            classify_error(Some(1), "some random error"),
            FfmpegErrorKind::Unknown
        );
    }

    #[test]
    fn hw_accel_is_retryable() {
        assert!(FfmpegErrorKind::HwAccelUnavailable.is_retryable());
        assert!(FfmpegErrorKind::HwAccelUnavailable.should_fallback_hw_accel());
    }

    #[test]
    fn invalid_input_not_retryable() {
        assert!(!FfmpegErrorKind::InvalidInput.is_retryable());
    }

    #[test]
    fn truncate_stderr_long() {
        let lines = (0..100).map(|i| format!("line {i}")).collect::<Vec<_>>();
        let stderr = lines.join("\n");
        let truncated = truncate_stderr(&stderr, 10);
        assert!(truncated.contains("(90 lines truncated)"));
        assert!(truncated.contains("line 99"));
    }

    #[test]
    fn truncate_stderr_short() {
        let stderr = "line 1\nline 2";
        assert_eq!(truncate_stderr(stderr, 10), stderr);
    }
}

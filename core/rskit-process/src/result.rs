//! Process execution result.

use std::time::Duration;

/// Result of a completed subprocess execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProcessResult {
    /// Process exit code. None if the process was killed.
    pub exit_code: Option<i32>,
    /// Captured standard output as a string.
    pub stdout: String,
    /// Captured standard output as raw bytes before lossy UTF-8 conversion.
    pub stdout_bytes: Vec<u8>,
    /// Captured standard error as a string.
    pub stderr: String,
    /// Captured standard error as raw bytes before lossy UTF-8 conversion.
    pub stderr_bytes: Vec<u8>,
    /// Whether stdout exceeded the configured capture limit.
    pub stdout_truncated: bool,
    /// Whether stderr exceeded the configured capture limit.
    pub stderr_truncated: bool,
    /// Total duration the process ran.
    pub duration: Duration,
    /// Whether the process was killed due to timeout.
    pub timed_out: bool,
    /// Whether the process was cancelled by the caller.
    pub cancelled: bool,
}

impl ProcessResult {
    /// Build a process result from captured output bytes.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "distinct completion fields of a public result; grouping them would introduce a new \
                  public parameter type solely to satisfy the lint"
    )]
    pub fn completed(
        exit_code: Option<i32>,
        stdout_bytes: Vec<u8>,
        stderr_bytes: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
        duration: Duration,
        timed_out: bool,
        cancelled: bool,
    ) -> Self {
        Self {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stdout_bytes,
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            stderr_bytes,
            stdout_truncated,
            stderr_truncated,
            duration,
            timed_out,
            cancelled,
        }
    }

    /// Check if the process exited successfully (exit code 0).
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::ProcessResult;
    /// use std::time::Duration;
    ///
    /// let result = ProcessResult::completed(
    ///     Some(0),
    ///     b"output".to_vec(),
    ///     Vec::new(),
    ///     false,
    ///     false,
    ///     Duration::from_secs(1),
    ///     false,
    ///     false,
    /// );
    ///
    /// assert!(result.success());
    /// ```
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Verify the process exited successfully, returning an error if not.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rskit_process::{ProcessConfig, ProcessSpec, run_with_cancel};
    /// use tokio_util::sync::CancellationToken;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let spec = ProcessSpec::new("echo").arg("hello");
    /// let result = run_with_cancel(&spec, &ProcessConfig::default(), CancellationToken::new()).await?;
    /// result.check()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn check(&self) -> crate::AppResult<&Self> {
        if self.cancelled {
            return Err(
                crate::AppError::new(crate::ErrorCode::Cancelled, "process cancelled")
                    .with_detail("cancelled", true),
            );
        }
        if self.timed_out {
            return Err(
                crate::AppError::new(crate::ErrorCode::Timeout, "process timed out")
                    .with_detail("timed_out", true),
            );
        }

        match self.exit_code {
            Some(0) => Ok(self),
            Some(code) => Err(crate::AppError::new(
                crate::ErrorCode::Internal,
                format!("process exited with code {}", code),
            )
            .with_detail("exit_code", code)),
            None => Err(
                crate::AppError::new(crate::ErrorCode::Internal, "process was killed")
                    .with_detail("killed", true),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn result(exit_code: Option<i32>, timed_out: bool, cancelled: bool) -> ProcessResult {
        ProcessResult::completed(
            exit_code,
            b"out".to_vec(),
            b"err".to_vec(),
            false,
            true,
            Duration::from_millis(5),
            timed_out,
            cancelled,
        )
    }

    #[test]
    fn completed_result_preserves_bytes_strings_flags_and_duration() {
        let result = result(Some(0), false, false);

        assert!(result.success());
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stdout_bytes, b"out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.stderr_bytes, b"err");
        assert!(!result.stdout_truncated);
        assert!(result.stderr_truncated);
        assert_eq!(result.duration, Duration::from_millis(5));
        assert!(std::ptr::eq(result.check().unwrap(), &result));
    }

    #[test]
    fn check_reports_cancelled_timeout_failed_and_killed_states() {
        assert_eq!(
            result(Some(0), false, true).check().unwrap_err().code(),
            ErrorCode::Cancelled
        );
        assert_eq!(
            result(Some(0), true, false).check().unwrap_err().code(),
            ErrorCode::Timeout
        );
        assert_eq!(
            result(Some(2), false, false).check().unwrap_err().code(),
            ErrorCode::Internal
        );
        assert_eq!(
            result(None, false, false).check().unwrap_err().code(),
            ErrorCode::Internal
        );
        assert!(!result(Some(2), false, false).success());
    }
}

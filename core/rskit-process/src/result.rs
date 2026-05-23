//! Process execution result.

use std::time::Duration;

/// Result of a completed subprocess execution.
#[derive(Debug, Clone)]
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
}

impl ProcessResult {
    /// Check if the process exited successfully (exit code 0).
    ///
    /// # Example
    ///
    /// ```
    /// use rskit_process::ProcessResult;
    /// use std::time::Duration;
    ///
    /// let result = ProcessResult {
    ///     exit_code: Some(0),
    ///     stdout: "output".to_string(),
    ///     stdout_bytes: b"output".to_vec(),
    ///     stderr: "".to_string(),
    ///     stderr_bytes: Vec::new(),
    ///     stdout_truncated: false,
    ///     stderr_truncated: false,
    ///     duration: Duration::from_secs(1),
    ///     timed_out: false,
    /// };
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
    /// use rskit_process::{Command, ProcessConfig, run_with_cancel};
    /// use tokio_util::sync::CancellationToken;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let cmd = Command::new("echo").arg("hello");
    /// let result = run_with_cancel(&cmd, &ProcessConfig::default(), CancellationToken::new()).await?;
    /// result.check()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn check(&self) -> crate::AppResult<&Self> {
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

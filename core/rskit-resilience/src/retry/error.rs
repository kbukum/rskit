//! Error returned when all retry attempts are exhausted.

use rskit_errors::AppError;

/// Error returned when all retry attempts are exhausted.
#[derive(Debug)]
pub struct RetryError {
    /// Total number of attempts made.
    pub attempts: usize,
    /// The error returned by the last attempt.
    pub last_error: AppError,
}

impl std::fmt::Display for RetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "all {} retry attempts failed; last: {}",
            self.attempts, self.last_error
        )
    }
}

impl std::error::Error for RetryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.last_error)
    }
}

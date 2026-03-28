use std::time::Duration;

use rskit_errors::{AppError, AppResult};

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

/// Exponential back-off retry policy with optional jitter.
///
/// # Example
///
/// ```rust
/// # use rskit_resilience::RetryPolicy;
/// # async fn example() {
/// let policy = RetryPolicy::new()
///     .with_max_attempts(3)
///     .with_initial_backoff(std::time::Duration::from_millis(100));
///
/// let result = policy.execute(|| async {
///     // your fallible operation
///     Ok::<_, rskit_errors::AppError>(42)
/// }).await;
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts before giving up (including the first call).
    pub max_attempts: usize,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound on any single backoff delay.
    pub max_backoff: Duration,
    /// Multiplier applied on each successive retry.
    pub backoff_factor: f64,
    /// Whether to add uniform jitter to each backoff delay.
    pub jitter: bool,
    /// Predicate that decides whether a given error is worth retrying.
    pub retry_if: fn(&AppError) -> bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_factor: 2.0,
            jitter: true,
            retry_if: |e| e.is_retryable(),
        }
    }
}

impl RetryPolicy {
    /// Create a new [`RetryPolicy`] with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of attempts (including the first call).
    #[must_use]
    pub fn with_max_attempts(mut self, n: usize) -> Self {
        self.max_attempts = n;
        self
    }

    /// Set the initial backoff delay before the first retry.
    #[must_use]
    pub fn with_initial_backoff(mut self, d: Duration) -> Self {
        self.initial_backoff = d;
        self
    }

    /// Set the upper bound on any single backoff delay.
    #[must_use]
    pub fn with_max_backoff(mut self, d: Duration) -> Self {
        self.max_backoff = d;
        self
    }

    /// Set the exponential backoff multiplication factor.
    #[must_use]
    pub fn with_backoff_factor(mut self, f: f64) -> Self {
        self.backoff_factor = f;
        self
    }

    /// Enable or disable uniform jitter on each backoff delay.
    #[must_use]
    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.jitter = enabled;
        self
    }

    /// Override the predicate used to decide whether an error is retryable.
    #[must_use]
    pub fn with_retry_if(mut self, f: fn(&AppError) -> bool) -> Self {
        self.retry_if = f;
        self
    }

    /// Execute `f`, retrying on retryable [`AppError`]s according to this policy.
    ///
    /// Returns `Ok(T)` on first success, or `Err(RetryError)` when exhausted.
    pub async fn execute<F, Fut, T>(&self, mut f: F) -> Result<T, RetryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            match f().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt >= self.max_attempts || !(self.retry_if)(&e) {
                        return Err(RetryError { attempts: attempt, last_error: e });
                    }
                    let delay = self.backoff(attempt);
                    tracing::debug!(
                        attempt,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "retrying after delay"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub(crate) fn backoff(&self, attempt: usize) -> Duration {
        // exp = factor^(attempt-1)
        let exp = self.backoff_factor.powi(attempt.saturating_sub(1) as i32);
        let base_ms = (self.initial_backoff.as_millis() as f64 * exp) as u64;
        let capped_ms = base_ms.min(self.max_backoff.as_millis() as u64);

        let ms = if self.jitter {
            // uniform jitter in [0.5x, 1.5x]
            let factor = 0.5 + rand::random::<f64>();
            (capped_ms as f64 * factor) as u64
        } else {
            capped_ms
        };
        Duration::from_millis(ms)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use rskit_errors::{AppError, ErrorCode};
    use super::*;

    fn make_policy() -> RetryPolicy {
        RetryPolicy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false)
    }

    #[tokio::test]
    async fn execute_succeeds_immediately_on_first_success() {
        let policy = make_policy();
        let result = policy.execute(|| async { Ok::<i32, AppError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn execute_retries_and_succeeds_on_second_attempt() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = make_policy();

        let result = policy.execute(|| {
            let counter = counter.clone();
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    Err(AppError::new(ErrorCode::ConnectionFailed, "test"))
                } else {
                    Ok(99)
                }
            }
        }).await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn execute_returns_err_after_exhausting_all_attempts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = make_policy(); // max_attempts = 3

        let result = policy.execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
            }
        }).await;

        assert!(result.is_err());
        let retry_err = result.unwrap_err();
        assert_eq!(retry_err.attempts, 3);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_does_not_retry_non_retryable_error() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = make_policy();

        let result = policy.execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::NotFound, "test"))
            }
        }).await;

        assert!(result.is_err());
        // Should have stopped after first attempt because error is not retryable
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_with_max_attempts_one_does_not_retry() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::new()
            .with_max_attempts(1)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false);

        let result = policy.execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
            }
        }).await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}

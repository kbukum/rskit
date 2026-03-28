use std::time::Duration;

use rskit_errors::{AppError, AppResult};

/// Error returned when all retry attempts are exhausted.
#[derive(Debug)]
pub struct RetryError {
    pub attempts: usize,
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
    pub max_attempts: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_factor: f64,
    pub jitter: bool,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_attempts(mut self, n: usize) -> Self {
        self.max_attempts = n;
        self
    }

    pub fn with_initial_backoff(mut self, d: Duration) -> Self {
        self.initial_backoff = d;
        self
    }

    pub fn with_max_backoff(mut self, d: Duration) -> Self {
        self.max_backoff = d;
        self
    }

    pub fn with_backoff_factor(mut self, f: f64) -> Self {
        self.backoff_factor = f;
        self
    }

    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.jitter = enabled;
        self
    }

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

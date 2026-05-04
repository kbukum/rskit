use std::sync::Arc;
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

/// Fixed retry backoff that uses the same delay for every retry attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantBackoff {
    /// Delay applied to every retry attempt.
    pub delay: Duration,
}

impl ConstantBackoff {
    /// Create a new constant backoff strategy.
    #[must_use]
    pub fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

/// Linear retry backoff that increases by a constant increment each attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearBackoff {
    /// Delay used for the first retry.
    pub initial_backoff: Duration,
    /// Increment added for each subsequent retry.
    pub increment: Duration,
    /// Upper bound applied to the computed delay.
    pub max_backoff: Duration,
}

impl LinearBackoff {
    /// Create a new linear backoff strategy.
    #[must_use]
    pub fn new(initial_backoff: Duration, increment: Duration, max_backoff: Duration) -> Self {
        Self {
            initial_backoff,
            increment,
            max_backoff,
        }
    }
}

/// Backoff algorithm used by a [`RetryPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackoffKind {
    /// Exponential backoff using `initial_backoff * backoff_factor^(attempt-1)`.
    Exponential,
    /// Fixed delay for every retry attempt.
    Constant,
    /// Linearly increasing delay using `initial_backoff + increment * (attempt-1)`.
    Linear,
}

/// Retry policy with configurable backoff and retry predicate.
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
///     Ok::<_, rskit_errors::AppError>(42)
/// }).await;
/// # }
/// ```
pub struct RetryPolicy {
    /// Maximum number of attempts before giving up (including the first call).
    pub max_attempts: usize,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound on any single backoff delay.
    pub max_backoff: Duration,
    /// Multiplier applied on each successive retry when using exponential backoff.
    pub backoff_factor: f64,
    /// Whether to add uniform jitter to each backoff delay.
    pub jitter: bool,
    /// Backoff algorithm applied between retry attempts.
    pub backoff_kind: BackoffKind,
    /// Linear increment applied when [`BackoffKind::Linear`] is selected.
    pub linear_increment: Duration,
    /// Predicate that decides whether a given error is worth retrying.
    /// When `None`, defaults to `AppError::is_retryable()`.
    #[allow(clippy::type_complexity)]
    pub retry_if: Option<Arc<dyn Fn(&AppError) -> bool + Send + Sync>>,
    /// Called after each failed attempt before the next backoff sleep.
    /// Arguments: `(attempt_number, error)`.
    #[allow(clippy::type_complexity)]
    pub on_retry: Option<Arc<dyn Fn(u32, &AppError) + Send + Sync>>,
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("initial_backoff", &self.initial_backoff)
            .field("max_backoff", &self.max_backoff)
            .field("backoff_factor", &self.backoff_factor)
            .field("jitter", &self.jitter)
            .field("backoff_kind", &self.backoff_kind)
            .field("linear_increment", &self.linear_increment)
            .field("retry_if", &self.retry_if.as_ref().map(|_| "<fn>"))
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl Clone for RetryPolicy {
    fn clone(&self) -> Self {
        Self {
            max_attempts: self.max_attempts,
            initial_backoff: self.initial_backoff,
            max_backoff: self.max_backoff,
            backoff_factor: self.backoff_factor,
            jitter: self.jitter,
            backoff_kind: self.backoff_kind,
            linear_increment: self.linear_increment,
            retry_if: self.retry_if.clone(),
            on_retry: self.on_retry.clone(),
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_factor: 2.0,
            jitter: true,
            backoff_kind: BackoffKind::Exponential,
            linear_increment: Duration::from_millis(100),
            retry_if: None,
            on_retry: None,
        }
    }
}

impl RetryPolicy {
    /// Create a new [`RetryPolicy`] with default settings.
    #[must_use]
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

    /// Use a fixed delay for every retry attempt.
    #[must_use]
    pub fn with_constant_backoff(mut self, backoff: ConstantBackoff) -> Self {
        self.backoff_kind = BackoffKind::Constant;
        self.initial_backoff = backoff.delay;
        self.max_backoff = backoff.delay;
        self
    }

    /// Use a linearly increasing retry delay.
    #[must_use]
    pub fn with_linear_backoff(mut self, backoff: LinearBackoff) -> Self {
        self.backoff_kind = BackoffKind::Linear;
        self.initial_backoff = backoff.initial_backoff;
        self.linear_increment = backoff.increment;
        self.max_backoff = backoff.max_backoff;
        self
    }

    /// Override the predicate used to decide whether an error is retryable.
    #[must_use]
    pub fn with_retry_if(mut self, f: impl Fn(&AppError) -> bool + Send + Sync + 'static) -> Self {
        self.retry_if = Some(Arc::new(f));
        self
    }

    /// Register a callback called after each failed attempt before the next
    /// backoff sleep. Arguments passed: `(attempt_number, error)`.
    #[must_use]
    pub fn with_on_retry(mut self, f: impl Fn(u32, &AppError) + Send + Sync + 'static) -> Self {
        self.on_retry = Some(Arc::new(f));
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
                    let should_retry = self
                        .retry_if
                        .as_ref()
                        .map(|predicate| predicate(&e))
                        .unwrap_or_else(|| e.is_retryable());
                    if attempt >= self.max_attempts || !should_retry {
                        return Err(RetryError {
                            attempts: attempt,
                            last_error: e,
                        });
                    }
                    if let Some(cb) = &self.on_retry {
                        cb(attempt as u32, &e);
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

    /// Return the retry delay for a one-based failed attempt number.
    #[must_use]
    pub fn backoff_delay(&self, attempt: usize) -> Duration {
        let base_delay = match self.backoff_kind {
            BackoffKind::Exponential => {
                let exp = self.backoff_factor.powi(attempt.saturating_sub(1) as i32);
                let base_ms = (self.initial_backoff.as_millis() as f64 * exp) as u64;
                Duration::from_millis(base_ms.min(self.max_backoff.as_millis() as u64))
            }
            BackoffKind::Constant => self.initial_backoff,
            BackoffKind::Linear => {
                let initial = self.initial_backoff.as_millis() as u64;
                let increment = self.linear_increment.as_millis() as u64;
                let computed = initial
                    .saturating_add(increment.saturating_mul(attempt.saturating_sub(1) as u64));
                Duration::from_millis(computed.min(self.max_backoff.as_millis() as u64))
            }
        };

        if self.jitter && !base_delay.is_zero() {
            let factor = 0.5 + rand::random::<f64>();
            Duration::from_millis((base_delay.as_millis() as f64 * factor) as u64)
        } else {
            base_delay
        }
    }

    pub(crate) fn backoff(&self, attempt: usize) -> Duration {
        self.backoff_delay(attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::{AppError, ErrorCode};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

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

        let result = policy
            .execute(|| {
                let counter = counter.clone();
                async move {
                    let attempt = counter.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        Err(AppError::new(ErrorCode::ConnectionFailed, "test"))
                    } else {
                        Ok(99)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn execute_returns_err_after_exhausting_all_attempts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = make_policy();

        let result = policy
            .execute(|| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
                }
            })
            .await;

        assert!(result.is_err());
        let retry_err = result.unwrap_err();
        assert_eq!(retry_err.attempts, 3);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn execute_does_not_retry_non_retryable_error() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = make_policy();

        let result = policy
            .execute(|| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, AppError>(AppError::new(ErrorCode::NotFound, "test"))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_with_max_attempts_one_does_not_retry() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::new()
            .with_max_attempts(1)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false);

        let result = policy
            .execute(|| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn constant_backoff_uses_same_delay() {
        let policy = RetryPolicy::new()
            .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(25)))
            .with_jitter(false);

        assert_eq!(policy.backoff(1), Duration::from_millis(25));
        assert_eq!(policy.backoff(3), Duration::from_millis(25));
    }

    #[test]
    fn linear_backoff_increases_until_capped() {
        let policy = RetryPolicy::new()
            .with_linear_backoff(LinearBackoff::new(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(20),
            ))
            .with_jitter(false);

        assert_eq!(policy.backoff(1), Duration::from_millis(10));
        assert_eq!(policy.backoff(2), Duration::from_millis(15));
        assert_eq!(policy.backoff(3), Duration::from_millis(20));
        assert_eq!(policy.backoff(6), Duration::from_millis(20));
    }

    #[test]
    fn public_backoff_delay_matches_policy_backoff() {
        let policy = RetryPolicy::new()
            .with_initial_backoff(Duration::from_millis(10))
            .with_max_backoff(Duration::from_millis(30))
            .with_jitter(false);

        assert_eq!(policy.backoff_delay(3), Duration::from_millis(30));
    }
}

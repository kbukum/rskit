//! The retry policy: backoff configuration, retry predicate, and execution loop.

use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult};

use super::backoff::{BackoffKind, ConstantBackoff, LinearBackoff};
use super::error::RetryError;
use super::preset::RetryPreset;

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
    /// Upper bound on total retry elapsed time, including the first attempt.
    pub max_elapsed_time: Duration,
    /// Multiplier applied on each successive retry when using exponential backoff.
    pub backoff_factor: f64,
    /// Whether to add uniform jitter to each backoff delay.
    pub jitter: bool,
    /// Backoff algorithm applied between retry attempts.
    pub backoff_kind: BackoffKind,
    /// Linear increment applied when [`BackoffKind::Linear`] is selected.
    pub linear_increment: Duration,
    /// Predicate that decides whether a given error is worth retrying. When `None`,
    /// defaults to `AppError::is_retryable()`.
    #[allow(clippy::type_complexity)]
    pub retry_if: Option<Arc<dyn Fn(&AppError) -> bool + Send + Sync>>,
    /// Called after each failed attempt before the next backoff sleep. Arguments:
    /// `(attempt_number, error)`.
    #[allow(clippy::type_complexity)]
    pub on_retry: Option<Arc<dyn Fn(u32, &AppError) + Send + Sync>>,
    /// Seed used to make jitter deterministic across runs.
    pub jitter_seed: Option<u64>,
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("initial_backoff", &self.initial_backoff)
            .field("max_backoff", &self.max_backoff)
            .field("max_elapsed_time", &self.max_elapsed_time)
            .field("backoff_factor", &self.backoff_factor)
            .field("jitter", &self.jitter)
            .field("backoff_kind", &self.backoff_kind)
            .field("linear_increment", &self.linear_increment)
            .field("retry_if", &self.retry_if.as_ref().map(|_| "<fn>"))
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<fn>"))
            .field("jitter_seed", &self.jitter_seed)
            .finish()
    }
}

impl Clone for RetryPolicy {
    fn clone(&self) -> Self {
        Self {
            max_attempts: self.max_attempts,
            initial_backoff: self.initial_backoff,
            max_backoff: self.max_backoff,
            max_elapsed_time: self.max_elapsed_time,
            backoff_factor: self.backoff_factor,
            jitter: self.jitter,
            backoff_kind: self.backoff_kind,
            linear_increment: self.linear_increment,
            retry_if: self.retry_if.clone(),
            on_retry: self.on_retry.clone(),
            jitter_seed: self.jitter_seed,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            max_elapsed_time: Duration::from_secs(30),
            backoff_factor: 2.0,
            jitter: true,
            backoff_kind: BackoffKind::Exponential,
            linear_increment: Duration::from_millis(100),
            retry_if: None,
            on_retry: None,
            jitter_seed: None,
        }
    }
}

impl RetryPolicy {
    /// Create a new [`RetryPolicy`] with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a policy from a named retry preset.
    #[must_use]
    pub fn from_preset(preset: RetryPreset) -> Self {
        preset.policy()
    }

    /// Create a short retry loop for local tests and latency-sensitive operations.
    #[must_use]
    pub fn fast() -> Self {
        RetryPreset::Fast.policy()
    }

    /// Create a balanced default policy for service-to-service calls.
    #[must_use]
    pub fn standard() -> Self {
        RetryPreset::Standard.policy()
    }

    /// Create a more tolerant policy for external network dependencies.
    #[must_use]
    pub fn external_service() -> Self {
        RetryPreset::ExternalService.policy()
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

    /// Set the total elapsed-time cap for all attempts and backoff sleeps.
    #[must_use]
    pub fn with_max_elapsed_time(mut self, d: Duration) -> Self {
        self.max_elapsed_time = d;
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

    /// Set a deterministic seed for retry jitter.
    #[must_use]
    pub const fn with_jitter_seed(mut self, seed: u64) -> Self {
        self.jitter_seed = Some(seed);
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

    /// Register a callback called after each failed attempt before the next backoff sleep.
    /// Arguments passed: `(attempt_number, error)`.
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
        if let Err(error) = self.validate() {
            return Err(RetryError {
                attempts: 0,
                last_error: error,
            });
        }

        let mut attempt = 0usize;
        let started = tokio::time::Instant::now();
        loop {
            let Some(remaining) = self.max_elapsed_time.checked_sub(started.elapsed()) else {
                return Err(RetryError {
                    attempts: attempt,
                    last_error: AppError::timeout("retry elapsed time"),
                });
            };
            if remaining.is_zero() {
                return Err(RetryError {
                    attempts: attempt,
                    last_error: AppError::timeout("retry elapsed time"),
                });
            }

            attempt += 1;
            match tokio::time::timeout(remaining, f()).await {
                Err(_) => {
                    return Err(RetryError {
                        attempts: attempt,
                        last_error: AppError::timeout("retry elapsed time"),
                    });
                }
                Ok(Ok(v)) => return Ok(v),
                Ok(Err(e)) => {
                    let should_retry = self
                        .retry_if
                        .as_ref()
                        .map(|predicate| predicate(&e))
                        .unwrap_or_else(|| e.is_retryable());
                    if attempt >= self.max_attempts
                        || !should_retry
                        || started.elapsed() >= self.max_elapsed_time
                    {
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
                    if started.elapsed().saturating_add(delay) >= self.max_elapsed_time {
                        return Err(RetryError {
                            attempts: attempt,
                            last_error: e,
                        });
                    }
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
            let jitter = self
                .jitter_seed
                .map(|seed| Self::deterministic_unit(seed, attempt))
                .unwrap_or_else(rand::random::<f64>);
            let factor = 0.5 + jitter;
            Duration::from_millis((base_delay.as_millis() as f64 * factor) as u64)
        } else {
            base_delay
        }
    }

    /// Validate retry policy limits.
    ///
    /// # Errors
    /// Returns an error when attempts or backoff parameters are invalid.
    pub fn validate(&self) -> AppResult<()> {
        if self.max_attempts == 0 {
            return Err(AppError::invalid_input(
                "max_attempts",
                "retry attempts must be greater than zero",
            ));
        }
        if !self.backoff_factor.is_finite() || self.backoff_factor <= 0.0 {
            return Err(AppError::invalid_input(
                "backoff_factor",
                "retry backoff factor must be finite and greater than zero",
            ));
        }
        Ok(())
    }

    pub(crate) fn backoff(&self, attempt: usize) -> Duration {
        self.backoff_delay(attempt)
    }

    fn deterministic_unit(seed: u64, attempt: usize) -> f64 {
        let mut value = seed ^ ((attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^= value >> 31;
        (value >> 11) as f64 / ((1_u64 << 53) as f64)
    }
}

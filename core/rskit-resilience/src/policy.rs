use std::future::Future;
use std::time::Duration;

use rskit_errors::{AppError, AppResult};

use crate::{
    Bulkhead, BulkheadConfig, CbConfig, CircuitBreaker, RateLimiter, RateLimiterConfig, RetryPolicy,
};

/// High-level resilience policy that composes multiple primitives.
#[derive(Clone, Default)]
pub struct Policy {
    retry: Option<RetryPolicy>,
    circuit_breaker: Option<CircuitBreaker>,
    bulkhead: Option<Bulkhead>,
    rate_limiter: Option<RateLimiter>,
    timeout: Option<Duration>,
}

impl Policy {
    /// Create an empty policy with no resilience primitives enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable retry handling.
    #[must_use]
    pub fn with_retry(mut self, config: RetryPolicy) -> Self {
        self.retry = Some(config);
        self
    }

    /// Enable circuit breaking.
    ///
    /// # Errors
    ///
    /// Returns an error when the circuit-breaker configuration is invalid.
    #[must_use = "builder methods return an updated policy; use the returned value"]
    pub fn with_circuit_breaker(self, config: CbConfig) -> AppResult<Self> {
        self.try_with_circuit_breaker(config)
    }

    /// Enable circuit breaking from a validated configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the circuit-breaker configuration is invalid.
    #[must_use = "builder methods return an updated policy; use the returned value"]
    pub fn try_with_circuit_breaker(mut self, config: CbConfig) -> AppResult<Self> {
        self.circuit_breaker = Some(CircuitBreaker::new(config)?);
        Ok(self)
    }

    /// Enable bulkhead concurrency limiting.
    ///
    /// # Errors
    ///
    /// Returns an error when the bulkhead configuration is invalid.
    #[must_use = "builder methods return an updated policy; use the returned value"]
    pub fn with_bulkhead(self, config: BulkheadConfig) -> AppResult<Self> {
        self.try_with_bulkhead(config)
    }

    /// Enable bulkhead concurrency limiting from a validated configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the bulkhead configuration is invalid.
    #[must_use = "builder methods return an updated policy; use the returned value"]
    pub fn try_with_bulkhead(mut self, config: BulkheadConfig) -> AppResult<Self> {
        self.bulkhead = Some(Bulkhead::new(config)?);
        Ok(self)
    }

    /// Enable rate limiting with a pre-built limiter.
    #[must_use]
    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = Some(rate_limiter);
        self
    }

    /// Enable rate limiting from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the rate-limiter configuration is invalid.
    #[must_use = "builder methods return an updated policy; use the returned value"]
    pub fn try_with_rate_limiter_config(mut self, config: RateLimiterConfig) -> AppResult<Self> {
        self.rate_limiter = Some(RateLimiter::from_config(config)?);
        Ok(self)
    }

    /// Enable an execution timeout for the entire policy invocation.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Execute an operation through the configured policy stack.
    ///
    /// The execution order is: rate limit → bulkhead → circuit breaker → timeout → retry → fn.
    pub async fn execute<F, Fut, T, E>(&self, mut f: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: From<AppError> + Into<AppError>,
    {
        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.check().map_err(E::from)?;
        }

        if let Some(bulkhead) = &self.bulkhead {
            bulkhead
                .execute(|| async {
                    execute_circuit_breaker(
                        self.circuit_breaker.as_ref(),
                        self.timeout,
                        self.retry.as_ref(),
                        &mut f,
                    )
                    .await
                    .map_err(Into::into)
                })
                .await
                .map_err(E::from)
        } else {
            execute_circuit_breaker(
                self.circuit_breaker.as_ref(),
                self.timeout,
                self.retry.as_ref(),
                &mut f,
            )
            .await
        }
    }
}

async fn execute_circuit_breaker<F, Fut, T, E>(
    circuit_breaker: Option<&CircuitBreaker>,
    timeout: Option<Duration>,
    retry: Option<&RetryPolicy>,
    f: &mut F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: From<AppError> + Into<AppError>,
{
    if let Some(circuit_breaker) = circuit_breaker {
        circuit_breaker
            .execute(|| async { execute_timeout(timeout, retry, f).await.map_err(Into::into) })
            .await
            .map_err(E::from)
    } else {
        execute_timeout(timeout, retry, f).await
    }
}

async fn execute_timeout<F, Fut, T, E>(
    timeout: Option<Duration>,
    retry: Option<&RetryPolicy>,
    f: &mut F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: From<AppError> + Into<AppError>,
{
    if let Some(timeout) = timeout {
        tokio::time::timeout(timeout, execute_retry(retry, f))
            .await
            .map_err(|_| E::from(AppError::timeout("resilience policy")))?
    } else {
        execute_retry(retry, f).await
    }
}

async fn execute_retry<F, Fut, T, E>(retry: Option<&RetryPolicy>, f: &mut F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: From<AppError> + Into<AppError>,
{
    if let Some(retry) = retry {
        retry.validate().map_err(E::from)?;
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            match f().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let retryable_error: AppError = err.into();
                    let should_retry = retry
                        .retry_if
                        .as_ref()
                        .map(|predicate| predicate(&retryable_error))
                        .unwrap_or_else(|| retryable_error.is_retryable());
                    if attempt >= retry.max_attempts || !should_retry {
                        return Err(E::from(retryable_error));
                    }
                    if let Some(callback) = &retry.on_retry {
                        callback(attempt as u32, &retryable_error);
                    }
                    tokio::time::sleep(retry.backoff(attempt)).await;
                }
            }
        }
    } else {
        f().await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use rskit_errors::{AppError, ErrorCode};

    use super::*;
    use crate::{
        BulkheadConfig, CbConfig, ConstantBackoff, LinearBackoff, RateLimiter, RateLimiterConfig,
    };

    #[tokio::test]
    async fn policy_retries_until_success() {
        let mut attempts = 0usize;
        let policy = Policy::new().with_retry(
            RetryPolicy::new()
                .with_max_attempts(3)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                .with_jitter(false),
        );

        let result = policy
            .execute(|| {
                attempts += 1;
                let attempt = attempts;
                async move {
                    if attempt == 1 {
                        Err::<u32, AppError>(AppError::connection_failed("upstream"))
                    } else {
                        Ok(7)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 7);
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn policy_circuit_breaker_state_is_shared_across_calls() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let policy = Policy::new()
            .with_circuit_breaker(CbConfig::new("shared").with_max_failures(1))
            .unwrap();

        let first = policy
            .execute(|| async { Err::<(), AppError>(AppError::new(ErrorCode::Internal, "boom")) })
            .await;
        assert!(first.is_err());

        let second = policy
            .execute(|| {
                let attempts = attempts.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), AppError>(())
                }
            })
            .await;

        assert!(second.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn policy_timeout_wraps_retry_block() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let policy = Policy::new()
            .with_retry(
                RetryPolicy::new()
                    .with_max_attempts(5)
                    .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(5)))
                    .with_jitter(false),
            )
            .with_timeout(Duration::from_millis(20));

        let result = policy
            .execute(|| {
                let attempts = attempts.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(15)).await;
                    Err::<(), AppError>(AppError::connection_failed("slow"))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), ErrorCode::Timeout);
        assert!(attempts.load(Ordering::SeqCst) < 5);
    }

    #[tokio::test]
    async fn policy_combines_outer_guards() {
        let policy = Policy::new()
            .try_with_rate_limiter_config(RateLimiterConfig::new("policy", 10, 2))
            .unwrap()
            .with_bulkhead(BulkheadConfig::new("policy", 1))
            .unwrap()
            .with_circuit_breaker(CbConfig::new("policy"))
            .unwrap()
            .with_retry(RetryPolicy::new().with_linear_backoff(LinearBackoff::new(
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(5),
            )));

        let result = policy.execute(|| async { Ok::<_, AppError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn policy_rejects_invalid_builder_configs() {
        assert!(
            Policy::new()
                .with_circuit_breaker(CbConfig::new("bad-cb").with_max_failures(0))
                .is_err()
        );
        assert!(
            Policy::new()
                .with_bulkhead(BulkheadConfig::new("bad-bulkhead", 0))
                .is_err()
        );
        assert!(
            Policy::new()
                .try_with_rate_limiter_config(RateLimiterConfig::new("bad-rate", 0, 1))
                .is_err()
        );
    }

    #[tokio::test]
    async fn policy_without_retry_calls_operation_once_and_propagates_error() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let policy = Policy::new();

        let result = policy
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), AppError>(AppError::connection_failed("upstream"))
                }
            })
            .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(result.unwrap_err().code(), ErrorCode::ConnectionFailed);
    }

    #[tokio::test]
    async fn policy_with_prebuilt_rate_limiter_enforces_burst() {
        let limiter = RateLimiter::from_config(RateLimiterConfig::new("prebuilt", 1, 1)).unwrap();
        let policy = Policy::new().with_rate_limiter(limiter);

        let first = policy.execute(|| async { Ok::<_, AppError>(1) }).await;
        assert_eq!(first.unwrap(), 1);

        let second = policy.execute(|| async { Ok::<_, AppError>(2) }).await;
        assert_eq!(second.unwrap_err().code(), ErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn policy_retry_invokes_on_retry_and_honors_retry_if() {
        let retries = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&retries);
        let policy = Policy::new().with_retry(
            RetryPolicy::new()
                .with_max_attempts(3)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                .with_jitter(false)
                .with_retry_if(|e: &AppError| e.code() == ErrorCode::InvalidInput)
                .with_on_retry(move |_attempt, _err| {
                    seen.fetch_add(1, Ordering::SeqCst);
                }),
        );

        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let result = policy
            .execute(|| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<(), AppError>(AppError::new(ErrorCode::InvalidInput, "retry me"))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(retries.load(Ordering::SeqCst), 2);
    }
}

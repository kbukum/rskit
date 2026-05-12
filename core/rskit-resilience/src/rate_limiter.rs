use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as GovRateLimiter};
use rskit_errors::{AppError, AppResult};
use tokio_util::sync::CancellationToken;

/// Configuration for constructing a [`RateLimiter`].
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Human-readable limiter name used in error details.
    pub name: String,
    /// Requests permitted per second.
    pub per_second: u32,
    /// Burst capacity.
    pub burst: u32,
}

impl RateLimiterConfig {
    /// Create a new rate-limiter configuration.
    #[must_use]
    pub fn new(name: impl Into<String>, per_second: u32, burst: u32) -> Self {
        Self {
            name: name.into(),
            per_second,
            burst,
        }
    }

    /// Set the steady-state requests per second.
    #[must_use]
    pub fn with_per_second(mut self, per_second: u32) -> Self {
        self.per_second = per_second;
        self
    }

    /// Set the burst capacity.
    #[must_use]
    pub fn with_burst(mut self, burst: u32) -> Self {
        self.burst = burst;
        self
    }
}

/// Token-bucket rate limiter backed by `governor`.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<DefaultDirectRateLimiter>,
    name: String,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("name", &self.name)
            .finish()
    }
}

impl RateLimiter {
    /// Create a rate limiter that allows `per_second` requests/second with a
    /// burst capacity of `burst`.
    #[must_use]
    pub fn new(name: impl Into<String>, per_second: u32, burst: u32) -> Self {
        let config = RateLimiterConfig::new(name, per_second, burst);
        Self::from_config(config)
    }

    /// Create a rate limiter from a configuration object.
    #[must_use]
    pub fn from_config(config: RateLimiterConfig) -> Self {
        let per_sec = NonZeroU32::new(config.per_second.max(1)).unwrap_or(NonZeroU32::MIN);
        let burst_size = NonZeroU32::new(config.burst.max(1)).unwrap_or(NonZeroU32::MIN);
        let quota = Quota::per_second(per_sec).allow_burst(burst_size);
        Self {
            inner: Arc::new(GovRateLimiter::direct(quota)),
            name: config.name,
        }
    }

    /// Non-blocking check: returns `Ok(())` if a token was acquired, or
    /// `Err(AppError::rate_limited())` if the bucket is empty.
    pub fn check(&self) -> AppResult<()> {
        self.inner
            .check()
            .map_err(|_| AppError::rate_limited().with_detail("rate_limiter", self.name.clone()))
    }

    /// Async wait: blocks until a token is available or `cancel` fires.
    pub async fn until_ready(&self, cancel: Option<CancellationToken>) -> AppResult<()> {
        match cancel {
            Some(token) => {
                tokio::select! {
                    _ = self.inner.until_ready() => Ok(()),
                    _ = token.cancelled() => {
                        Err(AppError::service_unavailable("rate limiter cancelled"))
                    }
                }
            }
            None => {
                self.inner.until_ready().await;
                Ok(())
            }
        }
    }
}

impl From<RateLimiterConfig> for RateLimiter {
    fn from(config: RateLimiterConfig) -> Self {
        Self::from_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn check_allows_up_to_burst_limit() {
        let rl = RateLimiter::new("test", 1, 5);
        for _ in 0..5 {
            assert!(rl.check().is_ok());
        }
    }

    #[tokio::test]
    async fn check_rejects_when_bucket_exhausted() {
        let rl = RateLimiter::new("test", 1, 3);
        for _ in 0..3 {
            let _ = rl.check();
        }
        let result = rl.check();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_returns_rate_limited_error_code() {
        use rskit_errors::ErrorCode;
        let rl = RateLimiter::new("test", 1, 1);
        let _ = rl.check();
        let err = rl.check().unwrap_err();
        assert_eq!(err.code, ErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn until_ready_cancels_when_token_cancelled() {
        let rl = RateLimiter::new("test", 1, 1);
        let _ = rl.check();

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        cancel_clone.cancel();

        let result = rl.until_ready(Some(cancel)).await;
        assert!(result.is_err());
    }

    #[test]
    fn from_config_builds_rate_limiter() {
        let limiter = RateLimiter::from_config(RateLimiterConfig::new("cfg", 10, 2));
        assert!(limiter.check().is_ok());
    }
}

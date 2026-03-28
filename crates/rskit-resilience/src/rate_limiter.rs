use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as GovRateLimiter};
use rskit_errors::{AppError, AppResult};
use tokio_util::sync::CancellationToken;

/// Token-bucket rate limiter backed by `governor`.
///
/// `governor` uses atomic operations (no mutex) and supports injectable clocks,
/// making it suitable for testing with fake time.
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
    pub fn new(name: impl Into<String>, per_second: u32, burst: u32) -> Self {
        let per_sec = NonZeroU32::new(per_second.max(1)).expect("per_second must be > 0");
        let burst_size = NonZeroU32::new(burst.max(1)).expect("burst must be > 0");
        let quota = Quota::per_second(per_sec).allow_burst(burst_size);
        Self {
            inner: Arc::new(GovRateLimiter::direct(quota)),
            name: name.into(),
        }
    }

    /// Non-blocking check: returns `Ok(())` if a token was acquired, or
    /// `Err(AppError::rate_limited())` if the bucket is empty.
    pub fn check(&self) -> AppResult<()> {
        self.inner.check().map_err(|_| {
            AppError::rate_limited().with_detail("rate_limiter", self.name.clone())
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn check_allows_up_to_burst_limit() {
        // 1 per second, burst of 5 — all 5 should succeed immediately
        let rl = RateLimiter::new("test", 1, 5);
        for _ in 0..5 {
            assert!(rl.check().is_ok());
        }
    }

    #[tokio::test]
    async fn check_rejects_when_bucket_exhausted() {
        // 1 per second, burst of 3
        let rl = RateLimiter::new("test", 1, 3);
        // Drain the burst
        for _ in 0..3 {
            let _ = rl.check();
        }
        // Next call should be rejected
        let result = rl.check();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_returns_rate_limited_error_code() {
        use rskit_errors::ErrorCode;
        let rl = RateLimiter::new("test", 1, 1);
        // Drain the one token
        let _ = rl.check();
        let err = rl.check().unwrap_err();
        assert_eq!(err.code, ErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn until_ready_cancels_when_token_cancelled() {
        use tokio_util::sync::CancellationToken;
        // Very slow limiter: 1/second, burst of 0 effectively — already drained
        let rl = RateLimiter::new("test", 1, 1);
        // Drain the single token so until_ready would wait forever
        let _ = rl.check();

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        // Cancel immediately
        cancel_clone.cancel();

        let result = rl.until_ready(Some(cancel)).await;
        assert!(result.is_err());
    }
}

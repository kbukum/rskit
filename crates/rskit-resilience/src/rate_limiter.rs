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

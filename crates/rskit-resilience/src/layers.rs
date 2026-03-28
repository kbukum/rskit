//! [`tower::Layer`] implementations wrapping each resilience primitive.
//!
//! These layers compose naturally with `tower::ServiceBuilder`:
//!
//! ```ignore
//! use tower::ServiceBuilder;
//! use rskit_resilience::layers::{RetryLayer, CircuitBreakerLayer};
//!
//! let svc = ServiceBuilder::new()
//!     .layer(CircuitBreakerLayer::new(cb))
//!     .layer(RetryLayer::new(policy))
//!     .service(my_base_service);
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

use crate::{Bulkhead, CircuitBreaker, RateLimiter, RetryPolicy};

// ─── Retry ──────────────────────────────────────────────────────────────────

/// Tower layer that retries failed requests according to a [`RetryPolicy`].
#[derive(Clone)]
pub struct RetryLayer(pub RetryPolicy);

impl RetryLayer {
    pub fn new(policy: RetryPolicy) -> Self {
        Self(policy)
    }
}

impl<S> tower::Layer<S> for RetryLayer {
    type Service = RetryService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RetryService { inner, policy: self.0.clone() }
    }
}

#[derive(Clone)]
pub struct RetryService<S> {
    inner: S,
    policy: RetryPolicy,
}

impl<S, Req> tower::Service<Req> for RetryService<S>
where
    S: tower::Service<Req, Error = AppError> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    Req: Clone + Send + 'static,
{
    type Response = S::Response;
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, AppError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        // Clone the base service once; then clone again per attempt.
        // This avoids capturing `req` by shared reference (which would
        // require `Req: Sync` for the future to be `Send`).
        let base = self.inner.clone();
        let policy = self.policy.clone();
        Box::pin(async move {
            let mut last_err: Option<AppError> = None;
            for attempt in 0..policy.max_attempts {
                let mut s = base.clone();
                let r = req.clone();
                match s.call(r).await {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        let retryable = (policy.retry_if)(&e);
                        if !retryable || attempt + 1 >= policy.max_attempts {
                            return Err(e);
                        }
                        let delay = policy.backoff(attempt + 1);
                        tracing::debug!(
                            attempt = attempt + 1,
                            delay_ms = delay.as_millis(),
                            error = %e,
                            "retrying after error"
                        );
                        tokio::time::sleep(delay).await;
                        last_err = Some(e);
                    }
                }
            }
            Err(last_err.unwrap_or_else(|| {
                AppError::new(rskit_errors::ErrorCode::Internal, "retry exhausted with no error")
            }))
        })
    }
}

// ─── Circuit Breaker ─────────────────────────────────────────────────────────

/// Tower layer that wraps a service with a [`CircuitBreaker`].
#[derive(Clone)]
pub struct CircuitBreakerLayer(pub CircuitBreaker);

impl CircuitBreakerLayer {
    pub fn new(cb: CircuitBreaker) -> Self {
        Self(cb)
    }
}

impl<S> tower::Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreakerService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreakerService { inner, cb: self.0.clone() }
    }
}

#[derive(Clone)]
pub struct CircuitBreakerService<S> {
    inner: S,
    cb: CircuitBreaker,
}

impl<S, Req> tower::Service<Req> for CircuitBreakerService<S>
where
    S: tower::Service<Req, Error = AppError> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    Req: Send + 'static,
{
    type Response = S::Response;
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, AppError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let mut svc = self.inner.clone();
        let cb = self.cb.clone();
        Box::pin(async move { cb.execute(|| svc.call(req)).await })
    }
}

// ─── Bulkhead ────────────────────────────────────────────────────────────────

/// Tower layer that limits concurrency via a [`Bulkhead`].
#[derive(Clone)]
pub struct BulkheadLayer(pub Bulkhead);

impl BulkheadLayer {
    pub fn new(bh: Bulkhead) -> Self {
        Self(bh)
    }
}

impl<S> tower::Layer<S> for BulkheadLayer {
    type Service = BulkheadService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        BulkheadService { inner, bh: self.0.clone() }
    }
}

#[derive(Clone)]
pub struct BulkheadService<S> {
    inner: S,
    bh: Bulkhead,
}

impl<S, Req> tower::Service<Req> for BulkheadService<S>
where
    S: tower::Service<Req, Error = AppError> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    Req: Send + 'static,
{
    type Response = S::Response;
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, AppError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let mut svc = self.inner.clone();
        let bh = self.bh.clone();
        Box::pin(async move { bh.execute(|| svc.call(req)).await })
    }
}

// ─── Rate Limit ──────────────────────────────────────────────────────────────

/// Tower layer that rate-limits a service via [`RateLimiter`].
#[derive(Clone)]
pub struct RateLimitLayer(pub RateLimiter);

impl RateLimitLayer {
    pub fn new(rl: RateLimiter) -> Self {
        Self(rl)
    }
}

impl<S> tower::Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService { inner, rl: self.0.clone() }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    rl: RateLimiter,
}

impl<S, Req> tower::Service<Req> for RateLimitService<S>
where
    S: tower::Service<Req, Error = AppError> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    Req: Send + 'static,
{
    type Response = S::Response;
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, AppError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let mut svc = self.inner.clone();
        let rl = self.rl.clone();
        Box::pin(async move {
            rl.check()?;
            svc.call(req).await
        })
    }
}

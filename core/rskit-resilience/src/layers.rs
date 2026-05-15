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
pub struct RetryLayer {
    policy: RetryPolicy,
}

impl RetryLayer {
    /// Create a new [`RetryLayer`] from the given policy.
    #[must_use]
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// Borrow the configured retry policy.
    #[must_use]
    pub const fn policy(&self) -> &RetryPolicy {
        &self.policy
    }
}

impl<S> tower::Layer<S> for RetryLayer {
    type Service = RetryService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RetryService {
            inner,
            policy: self.policy.clone(),
        }
    }
}

/// Tower service that retries failed requests using a [`RetryPolicy`].
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
            policy
                .execute(move || {
                    let mut s = base.clone();
                    let r = req.clone();
                    async move { s.call(r).await }
                })
                .await
                .map_err(|err| err.last_error)
        })
    }
}

// ─── Circuit Breaker ─────────────────────────────────────────────────────────

/// Tower layer that wraps a service with a [`CircuitBreaker`].
#[derive(Clone)]
pub struct CircuitBreakerLayer {
    breaker: CircuitBreaker,
}

impl CircuitBreakerLayer {
    /// Create a new [`CircuitBreakerLayer`] from the given breaker.
    #[must_use]
    pub fn new(cb: CircuitBreaker) -> Self {
        Self { breaker: cb }
    }

    /// Borrow the configured circuit breaker.
    #[must_use]
    pub const fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }
}

impl<S> tower::Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreakerService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreakerService {
            inner,
            cb: self.breaker.clone(),
        }
    }
}

/// Tower service that gates calls through a [`CircuitBreaker`].
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
pub struct BulkheadLayer {
    bulkhead: Bulkhead,
}

impl BulkheadLayer {
    /// Create a new [`BulkheadLayer`] from the given bulkhead.
    #[must_use]
    pub fn new(bh: Bulkhead) -> Self {
        Self { bulkhead: bh }
    }

    /// Borrow the configured bulkhead.
    #[must_use]
    pub const fn bulkhead(&self) -> &Bulkhead {
        &self.bulkhead
    }
}

impl<S> tower::Layer<S> for BulkheadLayer {
    type Service = BulkheadService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        BulkheadService {
            inner,
            bh: self.bulkhead.clone(),
        }
    }
}

/// Tower service that limits concurrency via a [`Bulkhead`].
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
pub struct RateLimitLayer {
    limiter: RateLimiter,
}

impl RateLimitLayer {
    /// Create a new [`RateLimitLayer`] from the given limiter.
    #[must_use]
    pub fn new(rl: RateLimiter) -> Self {
        Self { limiter: rl }
    }

    /// Borrow the configured rate limiter.
    #[must_use]
    pub const fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }
}

impl<S> tower::Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            rl: self.limiter.clone(),
        }
    }
}

// ─── Timeout ─────────────────────────────────────────────────────────────────

/// Tower layer that bounds each service call by a timeout.
#[derive(Debug, Clone, Copy)]
pub struct TimeoutLayer {
    timeout: std::time::Duration,
}

impl TimeoutLayer {
    /// Create a timeout layer with a finite timeout.
    #[must_use]
    pub const fn new(timeout: std::time::Duration) -> Self {
        Self { timeout }
    }

    /// Return the configured timeout.
    #[must_use]
    pub const fn timeout(&self) -> std::time::Duration {
        self.timeout
    }
}

impl<S> tower::Layer<S> for TimeoutLayer {
    type Service = TimeoutService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimeoutService {
            inner,
            timeout: self.timeout,
        }
    }
}

/// Tower service that applies a finite timeout to each call.
#[derive(Debug, Clone)]
pub struct TimeoutService<S> {
    inner: S,
    timeout: std::time::Duration,
}

impl<S, Req> tower::Service<Req> for TimeoutService<S>
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
        let timeout = self.timeout;
        Box::pin(async move {
            tokio::time::timeout(timeout, svc.call(req))
                .await
                .map_err(|_| AppError::timeout("resilience timeout"))?
        })
    }
}

/// Tower service that rate-limits calls using a [`RateLimiter`].
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use parking_lot::Mutex;
    use rskit_errors::{AppError, AppResult, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use crate::{
        Bulkhead, BulkheadConfig, CbConfig, CircuitBreaker, RateLimiter, RetryPolicy,
        layers::{BulkheadLayer, CircuitBreakerLayer, RateLimitLayer, RetryLayer, TimeoutLayer},
    };

    // ── Helper: service_fn that counts calls and returns a preset result ───────

    #[allow(dead_code)]
    fn counting_service(
        results: Arc<Mutex<Vec<AppResult<i32>>>>,
    ) -> impl Service<
        i32,
        Response = i32,
        Error = AppError,
        Future = std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<i32>> + Send>>,
    > + Clone {
        let results = results.clone();
        tower::service_fn(move |_req: i32| {
            let results = results.clone();
            let result = results.lock().remove(0);
            Box::pin(async move { result })
                as std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<i32>> + Send>>
        })
    }

    // ── RetryLayer ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn retry_layer_succeeds_on_first_try() {
        let policy = RetryPolicy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false);

        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req * 2) });
        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(svc);

        let result = svc.ready().await.unwrap().call(5).await;
        assert_eq!(result.unwrap(), 10);
    }

    #[tokio::test]
    async fn retry_layer_retries_and_succeeds() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false);

        let c = counter.clone();
        let svc = tower::service_fn(move |_req: i32| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(AppError::new(ErrorCode::ConnectionFailed, "transient"))
                } else {
                    Ok(42)
                }
            }
        });
        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(svc);

        let result = svc.ready().await.unwrap().call(0).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_layer_fails_after_exhausting_attempts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let policy = RetryPolicy::new()
            .with_max_attempts(2)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false);

        let c = counter.clone();
        let svc = tower::service_fn(move |_req: i32| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "always fails"))
            }
        });
        let mut svc = ServiceBuilder::new()
            .layer(RetryLayer::new(policy))
            .service(svc);

        let result = svc.ready().await.unwrap().call(0).await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    // ── CircuitBreakerLayer ────────────────────────────────────────────────────

    #[tokio::test]
    async fn cb_layer_passes_through_success() {
        let cb = CircuitBreaker::new(CbConfig::new("test-layer-cb").with_max_failures(3));
        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req + 1) });
        let mut svc = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(cb))
            .service(svc);

        let result = svc.ready().await.unwrap().call(9).await;
        assert_eq!(result.unwrap(), 10);
    }

    #[tokio::test]
    async fn cb_layer_opens_and_rejects() {
        let cb = CircuitBreaker::new(CbConfig::new("test-layer-cb").with_max_failures(2));
        let svc = tower::service_fn(|_req: i32| async {
            Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
        });
        let mut svc = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(cb.clone()))
            .service(svc);

        // Trip the breaker
        for _ in 0..2 {
            let _ = svc.ready().await.unwrap().call(0).await;
        }

        // Now the CB is open — next call is rejected without hitting the service
        let result = svc.ready().await.unwrap().call(0).await;
        assert!(result.is_err());
        assert_eq!(cb.state(), crate::CbState::Open);
    }

    // ── BulkheadLayer ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn bulkhead_layer_passes_through() {
        let bh = Bulkhead::new(BulkheadConfig::new("test", 4));
        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut svc = ServiceBuilder::new()
            .layer(BulkheadLayer::new(bh))
            .service(svc);

        let result = svc.ready().await.unwrap().call(7).await;
        assert_eq!(result.unwrap(), 7);
    }

    // ── RateLimitLayer ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn rate_limit_layer_allows_first_call() {
        let rl = RateLimiter::new("test", 10, 5).unwrap();
        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut svc = ServiceBuilder::new()
            .layer(RateLimitLayer::new(rl))
            .service(svc);

        let result = svc.ready().await.unwrap().call(3).await;
        assert_eq!(result.unwrap(), 3);
    }

    #[tokio::test]
    async fn rate_limit_layer_rejects_when_exhausted() {
        // burst=1 — only 1 token available
        let rl = RateLimiter::new("test", 1, 1).unwrap();
        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut svc = ServiceBuilder::new()
            .layer(RateLimitLayer::new(rl))
            .service(svc);

        // First call consumes the token
        let _ = svc.ready().await.unwrap().call(1).await;
        // Second call immediately hits the empty bucket
        let result = svc.ready().await.unwrap().call(2).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn timeout_layer_bounds_slow_service() {
        let svc = tower::service_fn(|_req: i32| async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<i32, AppError>(1)
        });
        let mut svc = ServiceBuilder::new()
            .layer(TimeoutLayer::new(Duration::from_millis(1)))
            .service(svc);

        let err = svc.ready().await.unwrap().call(0).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Timeout);
    }
}

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
    /// Create a new [`RetryLayer`] from the given policy.
    pub fn new(policy: RetryPolicy) -> Self {
        Self(policy)
    }
}

impl<S> tower::Layer<S> for RetryLayer {
    type Service = RetryService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RetryService {
            inner,
            policy: self.0.clone(),
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
            let mut last_err: Option<AppError> = None;
            for attempt in 0..policy.max_attempts {
                let mut s = base.clone();
                let r = req.clone();
                match s.call(r).await {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        let retryable = if let Some(ref pred) = policy.retry_if {
                            pred(&e)
                        } else {
                            e.is_retryable()
                        };
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
                AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    "retry exhausted with no error",
                )
            }))
        })
    }
}

// ─── Circuit Breaker ─────────────────────────────────────────────────────────

/// Tower layer that wraps a service with a [`CircuitBreaker`].
#[derive(Clone)]
pub struct CircuitBreakerLayer(pub CircuitBreaker);

impl CircuitBreakerLayer {
    /// Create a new [`CircuitBreakerLayer`] from the given breaker.
    pub fn new(cb: CircuitBreaker) -> Self {
        Self(cb)
    }
}

impl<S> tower::Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreakerService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreakerService {
            inner,
            cb: self.0.clone(),
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
pub struct BulkheadLayer(pub Bulkhead);

impl BulkheadLayer {
    /// Create a new [`BulkheadLayer`] from the given bulkhead.
    pub fn new(bh: Bulkhead) -> Self {
        Self(bh)
    }
}

impl<S> tower::Layer<S> for BulkheadLayer {
    type Service = BulkheadService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        BulkheadService {
            inner,
            bh: self.0.clone(),
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
pub struct RateLimitLayer(pub RateLimiter);

impl RateLimitLayer {
    /// Create a new [`RateLimitLayer`] from the given limiter.
    pub fn new(rl: RateLimiter) -> Self {
        Self(rl)
    }
}

impl<S> tower::Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            rl: self.0.clone(),
        }
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

    use rskit_errors::{AppError, AppResult, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use crate::{
        Bulkhead, BulkheadConfig, CbConfig, CircuitBreaker, RateLimiter, RetryPolicy,
        layers::{BulkheadLayer, CircuitBreakerLayer, RateLimitLayer, RetryLayer},
    };

    // ── Helper: service_fn that counts calls and returns a preset result ───────

    #[allow(dead_code)]
    fn counting_service(
        results: Arc<std::sync::Mutex<Vec<AppResult<i32>>>>,
    ) -> impl Service<
        i32,
        Response = i32,
        Error = AppError,
        Future = std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<i32>> + Send>>,
    > + Clone {
        let results = results.clone();
        tower::service_fn(move |_req: i32| {
            let results = results.clone();
            let result = results.lock().unwrap().remove(0);
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
        let rl = RateLimiter::new("test", 10, 5);
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
        let rl = RateLimiter::new("test", 1, 1);
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
}

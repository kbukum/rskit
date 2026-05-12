use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;
pub use rskit_resilience::{CircuitBreaker, RateLimiter, RetryPolicy};
use tower::{Layer, Service};

/// Resilience configuration for a provider layer.
#[derive(Clone, Default)]
pub struct ResilienceConfig {
    /// Optional retry policy applied to each call.
    pub retry: Option<RetryPolicy>,
    /// Optional circuit breaker guarding the inner service.
    pub circuit_breaker: Option<CircuitBreaker>,
    /// Optional rate limiter checked before every call.
    pub rate_limiter: Option<RateLimiter>,
}

impl ResilienceConfig {
    /// Create an empty [`ResilienceConfig`] with no primitives enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable retry with the given policy.
    pub fn with_retry(mut self, p: RetryPolicy) -> Self {
        self.retry = Some(p);
        self
    }

    /// Enable circuit breaker protection.
    pub fn with_circuit_breaker(mut self, cb: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(cb);
        self
    }

    /// Enable rate limiting.
    pub fn with_rate_limiter(mut self, rl: RateLimiter) -> Self {
        self.rate_limiter = Some(rl);
        self
    }
}

/// Tower layer combining retry + circuit breaker + rate limiter.
#[derive(Clone)]
pub struct ResilienceLayer(pub ResilienceConfig);

impl ResilienceLayer {
    /// Create a new [`ResilienceLayer`] from the given config.
    pub fn new(config: ResilienceConfig) -> Self {
        Self(config)
    }
}

impl<S> Layer<S> for ResilienceLayer {
    type Service = ResilienceService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        ResilienceService {
            inner,
            config: self.0.clone(),
        }
    }
}

/// Tower service that applies retry, circuit-breaker, and rate-limit logic.
#[derive(Clone)]
pub struct ResilienceService<S> {
    inner: S,
    config: ResilienceConfig,
}

impl<S, Req> Service<Req> for ResilienceService<S>
where
    S: Service<Req, Error = AppError> + Clone + Send + 'static,
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
        let inner = self.inner.clone();
        let config = self.config.clone();

        Box::pin(async move {
            // Execution order: rate limit → circuit breaker → retry → inner service

            // 1. Rate Limiting (cheapest)
            if let Some(rl) = &config.rate_limiter {
                rl.check()?;
            }

            // 2. Define the combined CB + inner service call
            let cb = config.circuit_breaker.clone();
            let svc = inner;
            let req_for_retry = req;

            let call_with_cb = move || {
                let mut s = svc.clone();
                let r = req_for_retry.clone();
                let cb_inner = cb.clone();
                async move {
                    if let Some(cb_instance) = cb_inner {
                        cb_instance.execute(|| s.call(r)).await
                    } else {
                        s.call(r).await
                    }
                }
            };

            // 3. Apply Retry logic if configured
            match &config.retry {
                Some(policy) => policy.execute(call_with_cb).await.map_err(|e| e.last_error),
                None => call_with_cb().await,
            }
        })
    }
}

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;
use rskit_resilience::{CircuitBreaker, RateLimiter, RetryPolicy};

/// Resilience configuration for a provider layer.
#[derive(Clone, Default)]
pub struct ResilienceConfig {
    pub retry: Option<RetryPolicy>,
    pub circuit_breaker: Option<CircuitBreaker>,
    pub rate_limiter: Option<RateLimiter>,
}

impl ResilienceConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_retry(mut self, p: RetryPolicy) -> Self {
        self.retry = Some(p);
        self
    }

    pub fn with_circuit_breaker(mut self, cb: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(cb);
        self
    }

    pub fn with_rate_limiter(mut self, rl: RateLimiter) -> Self {
        self.rate_limiter = Some(rl);
        self
    }
}

/// Tower layer combining retry + circuit breaker + rate limiter.
#[derive(Clone)]
pub struct ResilienceLayer(pub ResilienceConfig);

impl ResilienceLayer {
    pub fn new(config: ResilienceConfig) -> Self {
        Self(config)
    }
}

impl<S> tower::Layer<S> for ResilienceLayer {
    type Service = ResilienceService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        ResilienceService { inner, config: self.0.clone() }
    }
}

#[derive(Clone)]
pub struct ResilienceService<S> {
    inner: S,
    config: ResilienceConfig,
}

impl<S, Req> tower::Service<Req> for ResilienceService<S>
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
        let mut svc = self.inner.clone();
        let config = self.config.clone();

        Box::pin(async move {
            // Rate limit first (cheapest, no upstream call)
            if let Some(rl) = &config.rate_limiter {
                rl.check()?;
            }

            // The actual call, wrapped in CB + optional retry
            let call = move || {
                let r = req.clone();
                let mut s = svc.clone();
                let cb = config.circuit_breaker.clone();
                async move {
                    if let Some(cb) = cb {
                        cb.execute(|| s.call(r)).await
                    } else {
                        s.call(r).await
                    }
                }
            };

            match &config.retry {
                Some(policy) => policy.execute(call).await.map_err(|e| e.last_error),
                None => call().await,
            }
        })
    }
}

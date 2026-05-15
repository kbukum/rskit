use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

use crate::RateLimiter;

/// Tower layer that rate-limits a service via [`RateLimiter`].
#[derive(Clone)]
pub struct RateLimitLayer {
    limiter: RateLimiter,
}

impl RateLimitLayer {
    /// Create a new [`RateLimitLayer`] from the given limiter.
    #[must_use]
    pub fn new(limiter: RateLimiter) -> Self {
        Self { limiter }
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
            limiter: self.limiter.clone(),
        }
    }
}

/// Tower service that rate-limits calls using a [`RateLimiter`].
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    limiter: RateLimiter,
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
        let clone = self.inner.clone();
        let mut service = std::mem::replace(&mut self.inner, clone);
        let limiter = self.limiter.clone();
        Box::pin(async move {
            limiter.check()?;
            service.call(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::{AppError, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use super::*;
    use crate::RateLimiter;

    #[tokio::test]
    async fn allows_first_call() {
        let limiter = RateLimiter::new("test", 10, 5).unwrap();
        let service = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut service = ServiceBuilder::new()
            .layer(RateLimitLayer::new(limiter))
            .service(service);

        let result = service.ready().await.unwrap().call(3).await;
        assert_eq!(result.unwrap(), 3);
    }

    #[tokio::test]
    async fn rejects_when_exhausted() {
        let limiter = RateLimiter::new("test", 1, 1).unwrap();
        let service = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut service = ServiceBuilder::new()
            .layer(RateLimitLayer::new(limiter))
            .service(service);

        let _ = service.ready().await.unwrap().call(1).await;
        let error = service.ready().await.unwrap().call(2).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::RateLimited);
    }
}

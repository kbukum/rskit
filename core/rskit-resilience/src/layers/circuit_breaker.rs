use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

use crate::CircuitBreaker;

/// Tower layer that wraps a service with a [`CircuitBreaker`].
#[derive(Clone)]
pub struct CircuitBreakerLayer {
    breaker: CircuitBreaker,
}

impl CircuitBreakerLayer {
    /// Create a new [`CircuitBreakerLayer`] from the given breaker.
    #[must_use]
    pub fn new(breaker: CircuitBreaker) -> Self {
        Self { breaker }
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
            breaker: self.breaker.clone(),
        }
    }
}

/// Tower service that gates calls through a [`CircuitBreaker`].
#[derive(Clone)]
pub struct CircuitBreakerService<S> {
    inner: S,
    breaker: CircuitBreaker,
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
        let clone = self.inner.clone();
        let mut service = std::mem::replace(&mut self.inner, clone);
        let breaker = self.breaker.clone();
        Box::pin(async move { breaker.execute(|| service.call(req)).await })
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::{AppError, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use super::*;
    use crate::{CbConfig, CbState};

    #[tokio::test]
    async fn passes_through_success() {
        let breaker = CircuitBreaker::new(CbConfig::new("test-layer-cb").with_max_failures(3));
        let service = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req + 1) });
        let mut service = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(breaker))
            .service(service);

        let result = service.ready().await.unwrap().call(9).await;
        assert_eq!(result.unwrap(), 10);
    }

    #[tokio::test]
    async fn opens_and_rejects() {
        let breaker = CircuitBreaker::new(CbConfig::new("test-layer-cb").with_max_failures(2));
        let service = tower::service_fn(|_req: i32| async {
            Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
        });
        let mut service = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(breaker.clone()))
            .service(service);

        for _ in 0..2 {
            let _ = service.ready().await.unwrap().call(0).await;
        }

        let result = service.ready().await.unwrap().call(0).await;
        assert!(result.is_err());
        assert_eq!(breaker.state(), CbState::Open);
    }
}

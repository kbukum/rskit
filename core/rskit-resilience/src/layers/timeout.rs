use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use rskit_errors::AppError;

/// Tower layer that bounds each service call by a timeout.
#[derive(Debug, Clone, Copy)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    /// Create a timeout layer with a finite timeout.
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Return the configured timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
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
    timeout: Duration,
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
        let mut service = self.inner.clone();
        let timeout = self.timeout;
        Box::pin(async move {
            tokio::time::timeout(timeout, service.call(req))
                .await
                .map_err(|_| AppError::timeout("resilience timeout"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rskit_errors::{AppError, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use super::*;

    #[tokio::test]
    async fn bounds_slow_service() {
        let service = tower::service_fn(|_req: i32| async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<i32, AppError>(1)
        });
        let mut service = ServiceBuilder::new()
            .layer(TimeoutLayer::new(Duration::from_millis(1)))
            .service(service);

        let error = service.ready().await.unwrap().call(0).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
    }
}

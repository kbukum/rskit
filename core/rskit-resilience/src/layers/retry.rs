use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

use crate::RetryPolicy;

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
        let base = self.inner.clone();
        let policy = self.policy.clone();
        Box::pin(async move {
            policy
                .execute(move || {
                    let mut service = base.clone();
                    let req = req.clone();
                    async move { service.call(req).await }
                })
                .await
                .map_err(|err| err.last_error)
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

    use rskit_errors::{AppError, ErrorCode};
    use tower::{Service, ServiceBuilder, ServiceExt};

    use super::*;

    fn policy() -> RetryPolicy {
        RetryPolicy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false)
    }

    #[tokio::test]
    async fn succeeds_on_first_try() {
        let service = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req * 2) });
        let mut service = ServiceBuilder::new()
            .layer(RetryLayer::new(policy()))
            .service(service);

        let result = service.ready().await.unwrap().call(5).await;
        assert_eq!(result.unwrap(), 10);
    }

    #[tokio::test]
    async fn retries_and_succeeds() {
        let counter = Arc::new(AtomicUsize::new(0));
        let attempts = counter.clone();
        let service = tower::service_fn(move |_req: i32| {
            let attempts = attempts.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(AppError::new(ErrorCode::ConnectionFailed, "transient"))
                } else {
                    Ok(42)
                }
            }
        });
        let mut service = ServiceBuilder::new()
            .layer(RetryLayer::new(policy()))
            .service(service);

        let result = service.ready().await.unwrap().call(0).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fails_after_exhausting_attempts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let attempts = counter.clone();
        let service = tower::service_fn(move |_req: i32| {
            let attempts = attempts.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "always fails"))
            }
        });
        let mut service = ServiceBuilder::new()
            .layer(RetryLayer::new(
                policy()
                    .with_max_attempts(2)
                    .with_initial_backoff(Duration::from_millis(1)),
            ))
            .service(service);

        let result = service.ready().await.unwrap().call(0).await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use rskit_errors::AppError;

/// Tower layer that logs request duration and success/failure.
#[derive(Clone, Default)]
pub struct LoggingLayer {
    pub provider_name: &'static str,
}

impl LoggingLayer {
    pub fn new(provider_name: &'static str) -> Self {
        Self { provider_name }
    }
}

impl<S> tower::Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        LoggingService { inner, provider_name: self.provider_name }
    }
}

#[derive(Clone)]
pub struct LoggingService<S> {
    inner: S,
    provider_name: &'static str,
}

impl<S, Req> tower::Service<Req> for LoggingService<S>
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
        let name = self.provider_name;
        Box::pin(async move {
            let start = Instant::now();
            tracing::debug!(provider = name, "request started");
            let result = svc.call(req).await;
            let elapsed_ms = start.elapsed().as_millis();
            match &result {
                Ok(_) => tracing::debug!(provider = name, elapsed_ms, "request succeeded"),
                Err(e) => tracing::warn!(
                    provider = name,
                    elapsed_ms,
                    error = %e,
                    "request failed"
                ),
            }
            result
        })
    }
}

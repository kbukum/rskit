use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

/// Tower layer that wraps each call in a [`tracing::Span`].
#[derive(Clone)]
pub struct TracingLayer {
    /// Name of the provider being wrapped, used as a span field.
    pub provider_name: &'static str,
}

impl TracingLayer {
    /// Create a new [`TracingLayer`] for the named provider.
    pub fn new(provider_name: &'static str) -> Self {
        Self { provider_name }
    }
}

impl<S> tower::Layer<S> for TracingLayer {
    type Service = TracingService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        TracingService {
            inner,
            provider_name: self.provider_name,
        }
    }
}

/// Tower service that wraps each call in a tracing span.
#[derive(Clone)]
pub struct TracingService<S> {
    inner: S,
    provider_name: &'static str,
}

impl<S, Req> tower::Service<Req> for TracingService<S>
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
            let span = tracing::info_span!("provider.call", provider = name);
            let _enter = span.enter();
            svc.call(req).await
        })
    }
}

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use rskit_errors::AppError;

use crate::Bulkhead;

/// Tower layer that limits concurrency via a [`Bulkhead`].
#[derive(Clone)]
pub struct BulkheadLayer {
    bulkhead: Bulkhead,
}

impl BulkheadLayer {
    /// Create a new [`BulkheadLayer`] from the given bulkhead.
    #[must_use]
    pub fn new(bulkhead: Bulkhead) -> Self {
        Self { bulkhead }
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
            bulkhead: self.bulkhead.clone(),
        }
    }
}

/// Tower service that limits concurrency via a [`Bulkhead`].
#[derive(Clone)]
pub struct BulkheadService<S> {
    inner: S,
    bulkhead: Bulkhead,
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
        let clone = self.inner.clone();
        let mut service = std::mem::replace(&mut self.inner, clone);
        let bulkhead = self.bulkhead.clone();
        Box::pin(async move { bulkhead.execute(|| service.call(req)).await })
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::AppError;
    use tower::{Service, ServiceBuilder, ServiceExt};

    use super::*;
    use crate::{Bulkhead, BulkheadConfig};

    #[tokio::test]
    async fn passes_through() {
        let bulkhead = Bulkhead::new(BulkheadConfig::new("test", 4)).unwrap();
        let layer = BulkheadLayer::new(bulkhead.clone());
        assert_eq!(layer.bulkhead().available(), bulkhead.available());
        let service = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let mut service = ServiceBuilder::new().layer(layer).service(service);

        let result = service.ready().await.unwrap().call(7).await;
        assert_eq!(result.unwrap(), 7);
    }
}

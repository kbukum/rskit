//! Bridge between `tower::Service` and [`RequestResponse`].
//!
//! Wraps any `tower::Service<I, Response=O, Error=AppError>` so it can be
//! used wherever `RequestResponse<I, O>` is expected.

use std::marker::PhantomData;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_errors::AppResult;
use tower::ServiceExt;

use crate::traits::{Provider, RequestResponse};

/// Wraps a `tower::Service` as a `RequestResponse` provider.
///
/// The inner service is kept behind a `Mutex` because `tower::Service::call`
/// requires `&mut self`.
pub struct TowerProvider<S, I, O> {
    name: &'static str,
    service: Arc<Mutex<S>>,
    _phantom: PhantomData<fn(I) -> O>,
}

impl<S, I, O> TowerProvider<S, I, O>
where
    S: tower::Service<I, Response = O, Error = rskit_errors::AppError> + Send + Clone + 'static,
    S::Future: Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
{
    /// Create a new [`TowerProvider`] wrapping `service` with the given `name`.
    pub fn new(name: &'static str, service: S) -> Self {
        Self {
            name,
            service: Arc::new(Mutex::new(service)),
            _phantom: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S, I, O> Provider for TowerProvider<S, I, O>
where
    S: tower::Service<I, Response = O, Error = rskit_errors::AppError> + Send + Clone + 'static,
    S::Future: Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait::async_trait]
impl<S, I, O> RequestResponse<I, O> for TowerProvider<S, I, O>
where
    S: tower::Service<I, Response = O, Error = rskit_errors::AppError> + Send + Clone + 'static,
    S::Future: Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
{
    async fn execute(&self, input: I) -> AppResult<O> {
        // Clone the inner service to avoid holding the lock across .await
        let mut svc = self.service.lock().clone();
        svc.ready().await?;
        svc.call(input).await
    }
}

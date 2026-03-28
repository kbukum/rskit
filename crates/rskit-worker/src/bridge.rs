//! Bridges between [`Handler`] and [`rskit_provider`] traits.
//!
//! - `from_provider` wraps a `RequestResponse<I,O>` as a `Handler<I,O>`.
//!   The handler ignores the event channel (the provider has no concept of
//!   intermediate events) and returns the provider's result directly.
//!
//! - `as_provider` wraps an `Arc<dyn Handler<I,O>>` behind a `RequestResponse`
//!   adapter that submits one task, awaits its result, and returns it.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use rskit_errors::AppResult;
use rskit_provider::traits::{Provider, RequestResponse};

use crate::event::Event;
use crate::handler::Handler;

// ---------------------------------------------------------------------------
// from_provider
// ---------------------------------------------------------------------------

struct ProviderHandler<I, O, P> {
    provider: Arc<P>,
    _ph: std::marker::PhantomData<fn(I) -> O>,
}

#[async_trait::async_trait]
impl<I, O, P> Handler<I, O> for ProviderHandler<I, O, P>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
    P: RequestResponse<I, O> + Send + Sync + 'static,
{
    async fn handle(
        &self,
        task: I,
        _emit: mpsc::Sender<Event<O>>,
        _cancel: CancellationToken,
    ) -> AppResult<O> {
        self.provider.execute(task).await
    }
}

/// Wrap a [`RequestResponse`] provider as a [`Handler`].
///
/// The handler passes no intermediate events — the provider produces only a
/// final result.
pub fn from_provider<I, O, P>(provider: Arc<P>) -> impl Handler<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
    P: RequestResponse<I, O> + Send + Sync + 'static,
{
    ProviderHandler { provider, _ph: std::marker::PhantomData }
}

// ---------------------------------------------------------------------------
// as_provider
// ---------------------------------------------------------------------------

struct HandlerProvider<I, O> {
    name: &'static str,
    handler: Arc<dyn Handler<I, O>>,
}

#[async_trait::async_trait]
impl<I, O> Provider for HandlerProvider<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait::async_trait]
impl<I, O> RequestResponse<I, O> for HandlerProvider<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    async fn execute(&self, input: I) -> AppResult<O> {
        let (emit_tx, mut emit_rx) = mpsc::channel::<Event<O>>(64);
        let cancel = CancellationToken::new();

        // Drain events in background — callers using the provider interface
        // have no event subscription.
        tokio::spawn(async move {
            while emit_rx.recv().await.is_some() {}
        });

        self.handler.handle(input, emit_tx, cancel).await
    }
}

/// Wrap an `Arc<dyn Handler<I,O>>` as a [`RequestResponse`] provider.
pub fn as_provider<I, O>(
    name: &'static str,
    handler: Arc<dyn Handler<I, O>>,
) -> impl RequestResponse<I, O>
where
    I: Send + 'static,
    O: Send + Clone + 'static,
{
    HandlerProvider { name, handler }
}

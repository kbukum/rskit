//! Convenience constructors for building providers from plain async functions.

use std::future::Future;
use std::marker::PhantomData;

use rskit_errors::AppResult;

use crate::traits::{Provider, RequestResponse, Sink};

// ─── request_response_fn ─────────────────────────────────────────────────────

struct FnRR<I, O, F> {
    name: &'static str,
    f: F,
    _ph: PhantomData<fn(I) -> O>,
}

#[async_trait::async_trait]
impl<I, O, F, Fut> Provider for FnRR<I, O, F>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait::async_trait]
impl<I, O, F, Fut> RequestResponse<I, O> for FnRR<I, O, F>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    async fn execute(&self, input: I) -> AppResult<O> {
        (self.f)(input).await
    }
}

/// Create a [`RequestResponse`] provider from an async function.
pub fn request_response_fn<I, O, F, Fut>(
    name: &'static str,
    f: F,
) -> impl RequestResponse<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    FnRR { name, f, _ph: PhantomData }
}

// ─── sink_fn ─────────────────────────────────────────────────────────────────

struct FnSink<I, F> {
    name: &'static str,
    f: F,
    _ph: PhantomData<fn(I)>,
}

#[async_trait::async_trait]
impl<I, F, Fut> Provider for FnSink<I, F>
where
    I: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait::async_trait]
impl<I, F, Fut> Sink<I> for FnSink<I, F>
where
    I: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    async fn send(&self, input: I) -> AppResult<()> {
        (self.f)(input).await
    }
}

/// Create a [`Sink`] provider from an async function.
pub fn sink_fn<I, F, Fut>(
    name: &'static str,
    f: F,
) -> impl Sink<I>
where
    I: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    FnSink { name, f, _ph: PhantomData }
}

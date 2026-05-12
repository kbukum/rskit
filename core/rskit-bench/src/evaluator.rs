//! Evaluator trait and adapters for bench.
//!
//! An [`Evaluator`] takes raw `Vec<u8>` input and produces a
//! [`Prediction`]. This is the core abstraction
//! that bench uses to run model evaluation.

use crate::types::Prediction;
use rskit_errors::AppResult;
use rskit_provider::RequestResponse;
use std::future::Future;
use std::pin::Pin;

/// A boxed future (used internally for closure-based evaluators).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// An evaluator that produces predictions from raw input bytes.
///
/// Standalone trait (not extending `Provider`) to ensure object safety
/// via `#[async_trait]` — enables `Box<dyn Evaluator<L>>`.
#[async_trait::async_trait]
pub trait Evaluator<L = String>: Send + Sync
where
    L: Send + 'static,
{
    /// Returns the evaluator's name.
    fn name(&self) -> &'static str;

    /// Non-blocking availability check.
    async fn is_available(&self) -> bool {
        true
    }

    /// Execute the evaluator on raw input and return a prediction.
    async fn evaluate(&self, input: Vec<u8>) -> AppResult<Prediction<L>>;
}

/// Wraps a closure as an [`Evaluator`].
///
/// # Example
/// ```rust,ignore
/// let eval = EvaluatorFunc::new("my-model", |input: Vec<u8>| {
///     Box::pin(async move {
///         Ok(Prediction { label: "positive".into(), score: 0.95, ..Default::default() })
///     })
/// });
/// ```
pub struct EvaluatorFunc<L = String> {
    name: &'static str,
    #[allow(clippy::type_complexity)]
    func: Box<dyn Fn(Vec<u8>) -> BoxFuture<'static, AppResult<Prediction<L>>> + Send + Sync>,
}

impl<L> EvaluatorFunc<L> {
    pub fn new<F>(name: &'static str, func: F) -> Self
    where
        F: Fn(Vec<u8>) -> BoxFuture<'static, AppResult<Prediction<L>>> + Send + Sync + 'static,
    {
        Self {
            name,
            func: Box::new(func),
        }
    }
}

#[async_trait::async_trait]
impl<L: Send + Sync + 'static> Evaluator<L> for EvaluatorFunc<L> {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn evaluate(&self, input: Vec<u8>) -> AppResult<Prediction<L>> {
        (self.func)(input).await
    }
}

/// Adapts any [`RequestResponse<Vec<u8>, Prediction<L>>`] into an [`Evaluator<L>`].
pub struct FromProvider<I, O, L, TI, TO>
where
    TI: Fn(Vec<u8>) -> I + Send + Sync,
    TO: Fn(O) -> Prediction<L> + Send + Sync,
{
    provider: Box<dyn RequestResponse<I, O> + Send + Sync>,
    to_input: TI,
    to_prediction: TO,
    _phantom: std::marker::PhantomData<L>,
}

impl<I, O, L, TI, TO> FromProvider<I, O, L, TI, TO>
where
    I: Send + 'static,
    O: Send + 'static,
    L: Send + 'static,
    TI: Fn(Vec<u8>) -> I + Send + Sync,
    TO: Fn(O) -> Prediction<L> + Send + Sync,
{
    pub fn new(
        provider: impl RequestResponse<I, O> + Send + Sync + 'static,
        to_input: TI,
        to_prediction: TO,
    ) -> Self {
        Self {
            provider: Box::new(provider),
            to_input,
            to_prediction,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<I, O, L, TI, TO> Evaluator<L> for FromProvider<I, O, L, TI, TO>
where
    I: Send + 'static,
    O: Send + 'static,
    L: Send + Sync + 'static,
    TI: Fn(Vec<u8>) -> I + Send + Sync,
    TO: Fn(O) -> Prediction<L> + Send + Sync,
{
    fn name(&self) -> &'static str {
        self.provider.name()
    }

    async fn is_available(&self) -> bool {
        self.provider.is_available().await
    }

    async fn evaluate(&self, input: Vec<u8>) -> AppResult<Prediction<L>> {
        let converted_input = (self.to_input)(input);
        let output = self.provider.execute(converted_input).await?;
        Ok((self.to_prediction)(output))
    }
}

use crate::types::StepProgress;
use rskit_errors::AppResult;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Callback for chain-level progress updates.
pub type ChainProgressFn = Arc<dyn Fn(StepProgress) + Send + Sync>;

pub(crate) type CleanupAction =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'static>> + Send>;

pub(crate) struct ChainState<O> {
    pub(crate) output: O,
    pub(crate) cleanups: Vec<CleanupAction>,
}

#[derive(Clone)]
pub(crate) struct ChainContext {
    pub(crate) progress: Option<ChainProgressFn>,
    pub(crate) cancel: CancellationToken,
}

pub(crate) type ChainRunner<I, O> = Arc<
    dyn Fn(I, ChainContext) -> Pin<Box<dyn Future<Output = AppResult<ChainState<O>>> + Send>>
        + Send
        + Sync,
>;

pub(crate) async fn run_cleanups(mut cleanups: Vec<CleanupAction>) -> AppResult<()> {
    while let Some(cleanup) = cleanups.pop() {
        cleanup().await?;
    }
    Ok(())
}

/// Executes a typed sequence of steps.
pub struct Chain<I, O> {
    step_count: usize,
    runner: ChainRunner<I, O>,
    _types: PhantomData<fn(I) -> O>,
}

impl<I, O> Chain<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub(crate) fn new(step_count: usize, runner: ChainRunner<I, O>) -> Self {
        Self {
            step_count,
            runner,
            _types: PhantomData,
        }
    }

    /// Execute the chain, short-circuiting on the first failed step.
    pub async fn execute(
        &self,
        input: I,
        progress: Option<ChainProgressFn>,
        cancel: CancellationToken,
    ) -> AppResult<O> {
        let state = (self.runner)(input, ChainContext { progress, cancel }).await?;
        Ok(state.output)
    }

    /// Number of steps in the chain.
    #[must_use]
    pub fn len(&self) -> usize {
        self.step_count
    }

    /// Whether the chain has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.step_count == 0
    }
}

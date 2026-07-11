use crate::types::StepProgress;
use rskit_errors::{AppError, AppResult};
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
    let mut cleanup_error: Option<AppError> = None;
    while let Some(cleanup) = cleanups.pop() {
        if let Err(error) = cleanup().await {
            cleanup_error = Some(match cleanup_error {
                Some(existing) => {
                    existing.context(format!("additional chain cleanup failed: {error}"))
                }
                None => error.context("chain cleanup failed"),
            });
        }
    }
    cleanup_error.map_or(Ok(()), Err)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

    #[tokio::test]
    async fn run_cleanups_collects_multiple_failures_in_lifo_order() {
        let cleanups: Vec<CleanupAction> = vec![
            Box::new(|| {
                Box::pin(async { Err(AppError::new(ErrorCode::Internal, "cleanup one failed")) })
            }),
            Box::new(|| {
                Box::pin(async { Err(AppError::new(ErrorCode::Internal, "cleanup two failed")) })
            }),
        ];

        let error = run_cleanups(cleanups)
            .await
            .expect_err("cleanup failures should be reported");

        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(error.message().contains("cleanup two failed"));
        assert!(error.message().contains("cleanup one failed"));
    }

    #[tokio::test]
    async fn run_cleanups_succeeds_when_all_actions_succeed() {
        let cleanups: Vec<CleanupAction> = vec![Box::new(|| Box::pin(async { Ok(()) }))];

        run_cleanups(cleanups)
            .await
            .expect("successful cleanups should pass");
    }
}

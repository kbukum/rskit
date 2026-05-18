use crate::executor::{Chain, ChainContext, ChainRunner, ChainState, CleanupAction, run_cleanups};
use crate::operation::{Step, StepContext};
use crate::types::{StepProgress, StepStatus};
use rskit_errors::{AppError, ErrorCode};
use std::marker::PhantomData;
use std::sync::Arc;

/// Fluent builder for typed sequential chains.
pub struct ChainBuilder<I, O> {
    step_count: usize,
    runner: ChainRunner<I, O>,
    _types: PhantomData<fn(I) -> O>,
}

impl<T> ChainBuilder<T, T>
where
    T: Send + 'static,
{
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self {
            step_count: 0,
            runner: Arc::new(|input, _context| {
                Box::pin(async move {
                    Ok(ChainState {
                        output: input,
                        cleanups: Vec::new(),
                    })
                })
            }),
            _types: PhantomData,
        }
    }
}

impl<T> Default for ChainBuilder<T, T>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> ChainBuilder<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Add a typed step to the chain.
    #[must_use]
    pub fn step<N>(self, step: Step<O, N>) -> ChainBuilder<I, N>
    where
        N: Send + 'static,
    {
        let previous = self.runner;
        let step = Arc::new(step);
        let step_index = self.step_count;
        let runner: ChainRunner<I, N> = Arc::new(move |input, context: ChainContext| {
            let previous = previous.clone();
            let step = step.clone();
            Box::pin(async move {
                let mut state = previous(input, context.clone()).await?;
                if context.cancel.is_cancelled() {
                    let mut error = cancelled_error(step.id());
                    if let Err(cleanup_error) = run_cleanups(state.cleanups).await {
                        error = error.context(format!("{cleanup_error}"));
                    }
                    return Err(error);
                }

                emit_progress(
                    &context,
                    step_index,
                    step.id(),
                    StepStatus::Running,
                    0,
                    None,
                );
                let step_context = StepContext::new(
                    context.cancel.clone(),
                    context.progress.as_ref().map(|progress| {
                        let progress = progress.clone();
                        let step_id = step.id().to_string();
                        Arc::new(move |percent, message| {
                            progress(StepProgress {
                                step_index,
                                step_id: step_id.clone(),
                                status: StepStatus::Running,
                                progress_percent: percent,
                                message,
                            });
                        }) as Arc<dyn Fn(u8, Option<String>) + Send + Sync>
                    }),
                );

                let output = match step.execute(state.output, step_context).await {
                    Ok(output) => output,
                    Err(error) => {
                        let mut error = error.context(format!("chain step `{}` failed", step.id()));
                        if let Err(cleanup_error) = run_cleanups(state.cleanups).await {
                            error = error.context(format!("{cleanup_error}"));
                        }
                        return Err(error);
                    }
                };

                emit_progress(
                    &context,
                    step_index,
                    step.id(),
                    StepStatus::Completed,
                    100,
                    None,
                );

                if let Some(cleanup) = step.cleanup() {
                    state
                        .cleanups
                        .push(Box::new(move || cleanup()) as CleanupAction);
                }

                Ok(ChainState {
                    output,
                    cleanups: state.cleanups,
                })
            })
        });

        ChainBuilder {
            step_count: self.step_count + 1,
            runner,
            _types: PhantomData,
        }
    }

    /// Build the typed chain executor.
    #[must_use]
    pub fn build(self) -> Chain<I, O> {
        Chain::new(self.step_count, self.runner)
    }
}

fn emit_progress(
    context: &ChainContext,
    step_index: usize,
    step_id: &str,
    status: StepStatus,
    progress_percent: u8,
    message: Option<String>,
) {
    if let Some(progress) = &context.progress {
        progress(StepProgress {
            step_index,
            step_id: step_id.to_string(),
            status,
            progress_percent,
            message,
        });
    }
}

fn cancelled_error(step_id: &str) -> AppError {
    AppError::new(ErrorCode::Cancelled, "chain cancelled").with_detail("step", step_id.to_string())
}

//! Sequential step-based executor with progress reporting and cancellation.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;

/// The type of the async step function.
pub type StepFn<T> =
    Box<dyn Fn(T) -> Pin<Box<dyn Future<Output = AppResult<T>> + Send>> + Send + Sync>;

/// A named step in an executable pipeline.
pub struct Step<T> {
    /// Unique identifier for this step.
    pub id: String,
    /// Human-readable name for progress reporting.
    pub name: String,
    /// The async function to execute.
    pub execute: StepFn<T>,
}

/// Progress report emitted before and after each step.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StepStatus {
    /// A step is about to start.
    Started {
        /// ID of the step.
        step_id: String,
    },
    /// A step completed successfully.
    Completed {
        /// ID of the step.
        step_id: String,
        /// Wall-clock time spent in this step.
        elapsed: Duration,
    },
    /// A step failed.
    Failed {
        /// ID of the step.
        step_id: String,
        /// Error description.
        error: String,
        /// Wall-clock time before the failure.
        elapsed: Duration,
    },
}

/// Sequential executor that runs [`Step`]s in order, reporting progress and
/// honouring a [`CancellationToken`].
pub struct StepExecutor<T> {
    steps: Vec<Step<T>>,
}

impl<T: Send + 'static> StepExecutor<T> {
    /// Create a new executor with the given steps.
    pub fn new(steps: Vec<Step<T>>) -> Self {
        Self { steps }
    }

    /// Run all steps sequentially.
    ///
    /// * Each step receives the output of the previous step (or `input` for the first).
    /// * `on_progress` is called with [`StepStatus::Started`] and [`StepStatus::Completed`]
    ///   (or [`StepStatus::Failed`]) for every step.
    /// * If `cancel` is triggered, the executor stops between steps and returns
    ///   [`ErrorCode::Cancelled`].
    pub async fn execute(
        &self,
        input: T,
        on_progress: impl Fn(StepStatus) + Send,
        cancel: CancellationToken,
    ) -> AppResult<T> {
        let mut current = input;

        for step in &self.steps {
            // Check cancellation between steps
            if cancel.is_cancelled() {
                return Err(AppError::new(ErrorCode::Cancelled, "pipeline cancelled"));
            }

            on_progress(StepStatus::Started {
                step_id: step.id.clone(),
            });

            let start = Instant::now();
            let fut = (step.execute)(current);

            // Race the step future against cancellation
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    let elapsed = start.elapsed();
                    on_progress(StepStatus::Failed {
                        step_id: step.id.clone(),
                        error: "cancelled".into(),
                        elapsed,
                    });
                    return Err(AppError::new(ErrorCode::Cancelled, "pipeline cancelled"));
                }
                r = fut => r,
            };

            let elapsed = start.elapsed();

            match result {
                Ok(value) => {
                    on_progress(StepStatus::Completed {
                        step_id: step.id.clone(),
                        elapsed,
                    });
                    current = value;
                }
                Err(e) => {
                    on_progress(StepStatus::Failed {
                        step_id: step.id.clone(),
                        error: e.to_string(),
                        elapsed,
                    });
                    return Err(e);
                }
            }
        }

        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(id: &str, transform: fn(i32) -> AppResult<i32>) -> Step<i32> {
        let id_owned = id.to_string();
        Step {
            id: id.to_string(),
            name: id.to_string(),
            execute: Box::new(move |val| {
                let _id = id_owned.clone();
                Box::pin(async move { transform(val) })
            }),
        }
    }

    #[tokio::test]
    async fn executes_steps_sequentially() {
        let steps = vec![
            make_step("add-one", |v| Ok(v + 1)),
            make_step("double", |v| Ok(v * 2)),
        ];
        let executor = StepExecutor::new(steps);
        let statuses = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let s = statuses.clone();

        let result = executor
            .execute(
                5,
                move |status| s.lock().push(status),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // (5 + 1) * 2 = 12
        assert_eq!(result, 12);

        let statuses = statuses.lock();
        assert_eq!(statuses.len(), 4); // Started + Completed for each step
        assert!(matches!(&statuses[0], StepStatus::Started { step_id } if step_id == "add-one"));
        assert!(
            matches!(&statuses[1], StepStatus::Completed { step_id, .. } if step_id == "add-one")
        );
        assert!(matches!(&statuses[2], StepStatus::Started { step_id } if step_id == "double"));
        assert!(
            matches!(&statuses[3], StepStatus::Completed { step_id, .. } if step_id == "double")
        );
    }

    #[tokio::test]
    async fn propagates_step_failure() {
        let steps = vec![
            make_step("ok", |v| Ok(v + 1)),
            make_step("fail", |_| {
                Err(AppError::new(ErrorCode::Internal, "step broke"))
            }),
            make_step("unreachable", Ok),
        ];
        let executor = StepExecutor::new(steps);

        let err = executor
            .execute(0, |_| {}, CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[tokio::test]
    async fn cancellation_stops_execution() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let steps = vec![
            make_step("first", |v| Ok(v + 1)),
            make_step("second", |v| Ok(v + 1)),
        ];
        let executor = StepExecutor::new(steps);

        // Cancel before execution
        token_clone.cancel();

        let err = executor.execute(0, |_| {}, token).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn empty_steps_returns_input() {
        let executor = StepExecutor::<i32>::new(vec![]);
        let result = executor
            .execute(42, |_| {}, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result, 42);
    }
}

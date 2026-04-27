use crate::operation::{ChainOperation, ProgressFn};
use crate::types::{ChainResult, StepProgress, StepResult, StepStatus};
use rskit_errors::AppResult;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn};

/// Callback for chain-level progress updates.
pub type ChainProgressFn = Arc<dyn Fn(StepProgress) + Send + Sync>;

/// Configuration for chain execution.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    /// Whether to run cleanup on completed steps when a later step fails.
    pub cleanup_on_failure: bool,
    /// Whether to skip remaining steps on failure (`true`) or continue (`false`).
    pub stop_on_failure: bool,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            cleanup_on_failure: true,
            stop_on_failure: true,
        }
    }
}

/// Executes a sequence of operations, passing each output as input to the next.
pub struct ChainExecutor {
    operations: Vec<Box<dyn ChainOperation>>,
    config: ChainConfig,
}

impl ChainExecutor {
    /// Create a new executor from a list of operations.
    pub fn new(operations: Vec<Box<dyn ChainOperation>>) -> Self {
        Self {
            operations,
            config: ChainConfig::default(),
        }
    }

    /// Override the default configuration.
    pub fn with_config(mut self, config: ChainConfig) -> Self {
        self.config = config;
        self
    }

    /// Execute all operations sequentially.
    #[instrument(skip_all, fields(steps = self.operations.len()))]
    pub async fn execute(
        &self,
        input: Value,
        progress: Option<ChainProgressFn>,
        cancel: CancellationToken,
    ) -> AppResult<ChainResult> {
        let chain_start = Instant::now();
        let total_steps = self.operations.len();
        let mut results: Vec<StepResult> = Vec::with_capacity(total_steps);
        let mut current_input = input;
        let mut failed = false;

        for (index, operation) in self.operations.iter().enumerate() {
            // Check cancellation before starting each step
            if cancel.is_cancelled() {
                for remaining_op in &self.operations[index..] {
                    results.push(StepResult {
                        step_id: remaining_op.id().to_string(),
                        status: StepStatus::Cancelled,
                        duration: std::time::Duration::ZERO,
                        output: Value::Null,
                        error: Some("chain cancelled".into()),
                    });
                }
                break;
            }

            // Skip remaining if a previous step failed and stop_on_failure is true
            if failed && self.config.stop_on_failure {
                results.push(StepResult {
                    step_id: operation.id().to_string(),
                    status: StepStatus::Skipped,
                    duration: std::time::Duration::ZERO,
                    output: Value::Null,
                    error: None,
                });
                continue;
            }

            let step_id = operation.id().to_string();
            let step_start = Instant::now();

            // Emit "running" progress
            if let Some(ref p) = progress {
                p(StepProgress {
                    step_index: index,
                    step_id: step_id.clone(),
                    status: StepStatus::Running,
                    progress_percent: 0,
                    message: None,
                });
            }

            // Create per-step progress callback that wraps chain-level callback
            let step_progress: ProgressFn = if let Some(ref p) = progress {
                let p = Arc::clone(p);
                let sid = step_id.clone();
                let idx = index;
                Box::new(move |pct, msg| {
                    p(StepProgress {
                        step_index: idx,
                        step_id: sid.clone(),
                        status: StepStatus::Running,
                        progress_percent: pct,
                        message: msg,
                    });
                })
            } else {
                Box::new(|_, _| {})
            };

            info!(step = %step_id, index, total_steps, "executing chain step");

            let result = operation
                .execute(current_input.clone(), step_progress, cancel.clone())
                .await;

            let duration = step_start.elapsed();

            match result {
                Ok(output) => {
                    if let Some(ref p) = progress {
                        p(StepProgress {
                            step_index: index,
                            step_id: step_id.clone(),
                            status: StepStatus::Completed,
                            progress_percent: 100,
                            message: None,
                        });
                    }

                    current_input = output.clone();
                    results.push(StepResult {
                        step_id,
                        status: StepStatus::Completed,
                        duration,
                        output,
                        error: None,
                    });
                }
                Err(e) => {
                    error!(step = %step_id, error = %e, "chain step failed");

                    if let Some(ref p) = progress {
                        p(StepProgress {
                            step_index: index,
                            step_id: step_id.clone(),
                            status: StepStatus::Failed,
                            progress_percent: 0,
                            message: Some(e.to_string()),
                        });
                    }

                    results.push(StepResult {
                        step_id,
                        status: StepStatus::Failed,
                        duration,
                        output: Value::Null,
                        error: Some(e.to_string()),
                    });
                    failed = true;
                }
            }
        }

        // Cleanup on failure: call cleanup on all completed steps in reverse order
        let all_completed = results.iter().all(|r| r.status == StepStatus::Completed);
        if !all_completed && self.config.cleanup_on_failure {
            warn!("chain failed, cleaning up completed steps");
            for result in results.iter().rev() {
                if result.status == StepStatus::Completed
                    && let Some(op) = self.operations.iter().find(|o| o.id() == result.step_id)
                {
                    op.cleanup(&result.output).await;
                }
            }
        }

        let total_duration = chain_start.elapsed();
        let final_output = if all_completed {
            results.last().map(|r| r.output.clone())
        } else {
            None
        };

        Ok(ChainResult {
            steps: results,
            total_duration,
            final_output,
            success: all_completed,
        })
    }
}

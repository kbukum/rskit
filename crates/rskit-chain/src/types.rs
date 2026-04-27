use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Status of a single step in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum StepStatus {
    /// Step has not started yet.
    Pending,
    /// Step is currently executing.
    Running,
    /// Step finished successfully.
    Completed,
    /// Step failed with an error.
    Failed,
    /// Step was skipped (e.g., due to a prior failure).
    Skipped,
    /// Step was cancelled via the cancellation token.
    Cancelled,
}

/// Progress update for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepProgress {
    /// Zero-based index of this step in the chain.
    pub step_index: usize,
    /// Unique identifier of the step.
    pub step_id: String,
    /// Current status of the step.
    pub status: StepStatus,
    /// Completion percentage (0–100).
    pub progress_percent: u8,
    /// Optional human-readable progress message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Result of a single step execution.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Unique identifier of the step.
    pub step_id: String,
    /// Final status of the step.
    pub status: StepStatus,
    /// Wall-clock time the step took.
    pub duration: Duration,
    /// Output value produced by the step (or `Value::Null` on failure).
    pub output: serde_json::Value,
    /// Error message if the step failed.
    pub error: Option<String>,
}

/// Overall chain execution result.
#[derive(Debug, Clone)]
pub struct ChainResult {
    /// Per-step results in execution order.
    pub steps: Vec<StepResult>,
    /// Total wall-clock time for the entire chain.
    pub total_duration: Duration,
    /// Final output from the last completed step, or `None` on failure.
    pub final_output: Option<serde_json::Value>,
    /// Whether all steps completed successfully.
    pub success: bool,
}

impl ChainResult {
    /// Number of steps that completed successfully.
    pub fn completed_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count()
    }

    /// Returns the first failed step, if any.
    pub fn failed_step(&self) -> Option<&StepResult> {
        self.steps.iter().find(|s| s.status == StepStatus::Failed)
    }
}

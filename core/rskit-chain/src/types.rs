use serde::{Deserialize, Serialize};

/// Status of a single step in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum StepStatus {
    /// Step is currently executing.
    Running,
    /// Step finished successfully.
    Completed,
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
    /// Completion percentage, clamped to 0..=100.
    pub progress_percent: u8,
    /// Optional human-readable progress message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

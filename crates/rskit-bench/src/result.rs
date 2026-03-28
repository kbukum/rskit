//! Result types for bench runs.
//!
//! These types represent the complete output of a benchmark evaluation,
//! designed for cross-language compatibility with gokit and pykit.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::schema;

/// Complete result of a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRunResult {
    /// Generated run identifier.
    pub id: String,
    /// Schema URL.
    #[serde(rename = "$schema", default = "schema::schema_url")]
    pub schema: String,
    /// Schema version string.
    #[serde(default = "schema::version")]
    pub version: String,
    /// Run start time (ISO 8601).
    pub timestamp: String,
    /// User-provided tag.
    #[serde(default)]
    pub tag: String,
    /// Total run duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Dataset metadata.
    pub dataset: DatasetInfo,
    /// Top-level metrics.
    #[serde(default)]
    pub metrics: Vec<MetricResult>,
    /// Per-branch results.
    #[serde(default)]
    pub branches: HashMap<String, BranchResult>,
    /// Per-sample results.
    #[serde(default)]
    pub samples: Vec<BenchSampleResult>,
    /// Optional visualization curves.
    #[serde(default)]
    pub curves: HashMap<String, serde_json::Value>,
}

/// Dataset metadata included in run results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    /// Dataset name from manifest.
    pub name: String,
    /// Dataset version.
    #[serde(default)]
    pub version: String,
    /// Total samples evaluated.
    #[serde(default)]
    pub sample_count: usize,
    /// Count of each label.
    #[serde(default)]
    pub label_distribution: HashMap<String, usize>,
}

/// Result of a single metric computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    /// Metric name.
    pub name: String,
    /// Primary scalar result.
    pub value: f64,
    /// Per-label or secondary metrics.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub values: HashMap<String, f64>,
    /// Complex metric structure (confusion matrix, ROC curve, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// Per-branch evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    /// Branch/evaluator name.
    pub name: String,
    /// Tiering level.
    #[serde(default)]
    pub tier: i32,
    /// Metrics for this branch.
    #[serde(default)]
    pub metrics: HashMap<String, f64>,
    /// Average confidence on correct predictions.
    #[serde(default)]
    pub avg_score_positive: f64,
    /// Average confidence on incorrect predictions.
    #[serde(default)]
    pub avg_score_negative: f64,
    /// Branch evaluation time in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Count of errors during evaluation.
    #[serde(default)]
    pub errors: usize,
}

/// Per-sample evaluation details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSampleResult {
    /// Sample ID.
    pub id: String,
    /// Ground-truth label.
    pub label: String,
    /// Predicted label.
    #[serde(default)]
    pub predicted: String,
    /// Prediction confidence score.
    #[serde(default)]
    pub score: f64,
    /// Whether prediction matched ground truth.
    #[serde(default)]
    pub correct: bool,
    /// Scores from all branches.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_scores: HashMap<String, f64>,
    /// Evaluation time in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Error message if evaluation failed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Lightweight run summary for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRunSummary {
    /// Run ID.
    pub id: String,
    /// When run executed (ISO 8601).
    pub timestamp: String,
    /// User tag.
    #[serde(default)]
    pub tag: String,
    /// Dataset name.
    #[serde(default)]
    pub dataset: String,
    /// F1 metric (if available).
    #[serde(default)]
    pub f1: f64,
}

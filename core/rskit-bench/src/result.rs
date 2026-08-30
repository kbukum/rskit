//! Result types for bench runs.
//!
//! These types represent the complete output of a benchmark evaluation,
//! serialized to the shared bench result schema for cross-language interchange.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use super::provenance::RunProvenance;
use super::schema;

/// Serializes a string-keyed map with its keys in sorted order.
///
/// The in-memory type stays a [`HashMap`] for fast keyed lookup; only the wire
/// form is ordered. Sorting makes the emitted JSON deterministic (a plain
/// `HashMap` serializes in arbitrary order) and matches the sibling kit's
/// sorted-key map output, so the two kits emit an interchangeable contract.
fn serialize_sorted_map<S, V>(map: &HashMap<String, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    V: Serialize,
{
    map.iter().collect::<BTreeMap<_, _>>().serialize(serializer)
}

/// Complete result of a benchmark run.
///
/// Marked `#[non_exhaustive]`: construct it inside the crate via the runner, or from
/// [`BenchRunResult::default`] plus field assignment, so new fields can be added
/// without breaking external constructors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "serialize_sorted_map"
    )]
    pub branches: HashMap<String, BranchResult>,
    /// Per-sample results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<BenchSampleResult>,
    /// Optional visualization curves.
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "serialize_sorted_map"
    )]
    pub curves: HashMap<String, serde_json::Value>,
    /// Reproducibility provenance (seed, commit, tool/host identity, dataset hash).
    #[serde(default)]
    pub provenance: RunProvenance,
}

/// Dataset metadata included in run results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default, serialize_with = "serialize_sorted_map")]
    pub label_distribution: HashMap<String, usize>,
}

/// Result of a single metric computation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricResult {
    /// Metric name.
    pub name: String,
    /// Primary scalar result.
    pub value: f64,
    /// Optimization direction of [`value`](Self::value) and of every entry in
    /// [`values`](Self::values) not overridden in [`directions`](Self::directions):
    /// whether higher or lower is better, or whether the metric is purely
    /// descriptive.
    ///
    /// Defaults to [`MetricDirection::HigherIsBetter`], so the many accuracy-style
    /// metrics for which higher is better are classified correctly by
    /// [`RunComparator`](crate::compare::RunComparator) without setting it explicitly.
    #[serde(default)]
    pub direction: MetricDirection,
    /// Per-key optimization direction override for entries in
    /// [`values`](Self::values) whose direction differs from
    /// [`direction`](Self::direction). A key absent from this map inherits
    /// `direction`, so a heterogeneous metric — a higher-is-better headline (F1,
    /// R²) alongside lower-is-better diagnostics (false-positive rate, residual
    /// sum of squares) — classifies every subvalue correctly instead of
    /// inheriting one direction for the whole map.
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "serialize_sorted_map"
    )]
    pub directions: HashMap<String, MetricDirection>,
    /// Per-label or secondary metrics.
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "serialize_sorted_map"
    )]
    pub values: HashMap<String, f64>,
    /// Complex metric structure (confusion matrix, ROC curve, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// Optimization direction of a metric: whether higher or lower values are
/// better, or whether the metric is descriptive with no preferred direction.
///
/// Run comparison uses this to classify a metric change as an improvement or a
/// regression. Without it, every increase would be treated as an improvement —
/// wrong for error metrics (lower is better) and meaningless for descriptive
/// metrics such as token usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MetricDirection {
    /// Higher values are better (accuracy, F1, AUC, nDCG). The default.
    #[default]
    HigherIsBetter,
    /// Lower values are better (error rates, loss, calibration error).
    LowerIsBetter,
    /// Descriptive metric with no optimization direction (token usage, counts).
    Neutral,
}

impl MetricDirection {
    /// Classifies a signed value delta (`new - old`) as an improvement in this
    /// direction. A [`Neutral`](Self::Neutral) metric is never an improvement.
    #[must_use]
    pub fn is_improvement(self, delta: f64) -> bool {
        match self {
            Self::HigherIsBetter => delta > 0.0,
            Self::LowerIsBetter => delta < 0.0,
            Self::Neutral => false,
        }
    }

    /// Classifies a signed value delta (`new - old`) as a regression in this
    /// direction. A [`Neutral`](Self::Neutral) metric never regresses.
    #[must_use]
    pub fn is_regression(self, delta: f64) -> bool {
        match self {
            Self::HigherIsBetter => delta < 0.0,
            Self::LowerIsBetter => delta > 0.0,
            Self::Neutral => false,
        }
    }
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
    #[serde(default, serialize_with = "serialize_sorted_map")]
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
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "serialize_sorted_map"
    )]
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

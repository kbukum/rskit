//! Curve data types for visualization.
//!
//! These types are populated by metrics
//! and stored in `BenchRunResult.curves` for visualization by report and viz sub-modules.

use serde::{Deserialize, Serialize};

/// Receiver Operating Characteristic curve data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocCurve {
    /// False Positive Rates.
    pub fpr: Vec<f64>,
    /// True Positive Rates.
    pub tpr: Vec<f64>,
    /// Classification thresholds.
    pub thresholds: Vec<f64>,
    /// Area Under Curve.
    pub auc: f64,
}

/// Precision-Recall curve data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecisionRecallCurve {
    /// Precision values sampled along the threshold sweep.
    pub precision: Vec<f64>,
    /// Recall values sampled along the threshold sweep.
    pub recall: Vec<f64>,
    /// Classification thresholds corresponding to the precision and recall values.
    pub thresholds: Vec<f64>,
}

/// Calibration curve: predicted probability vs actual frequency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationCurve {
    /// Mean predicted probability for each calibration bin.
    pub predicted_probability: Vec<f64>,
    /// Observed positive-label frequency for each calibration bin.
    pub actual_frequency: Vec<f64>,
    /// Number of samples assigned to each calibration bin.
    pub bin_count: Vec<usize>,
}

/// Full N×N confusion matrix with labels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfusionMatrixDetail {
    /// Class labels.
    pub labels: Vec<String>,
    /// N×N matrix (row=actual, col=predicted).
    pub matrix: Vec<Vec<i64>>,
    /// Orientation description.
    #[serde(default = "default_orientation")]
    pub orientation: String,
}

fn default_orientation() -> String {
    "row=actual, col=predicted".to_string()
}

/// Score distribution for a single label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreDistribution {
    /// Label for this distribution.
    pub label: String,
    /// Histogram bin edges.
    pub bins: Vec<f64>,
    /// Counts per bin.
    pub counts: Vec<usize>,
}

/// Classification metrics at a specific threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdPoint {
    /// Classification threshold used for this point.
    pub threshold: f64,
    /// Precision measured at this threshold.
    pub precision: f64,
    /// Recall measured at this threshold.
    pub recall: f64,
    /// F1 score measured at this threshold.
    pub f1: f64,
    /// Accuracy measured at this threshold.
    pub accuracy: f64,
}

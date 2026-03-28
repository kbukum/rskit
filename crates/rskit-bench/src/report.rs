//! Report generation for bench runs.

use crate::metrics::ThresholdMetrics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete result of a bench run (legacy format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run_id: String,
    pub timestamp: String,
    #[serde(default)]
    pub tag: String,
    pub dataset_name: String,
    pub sample_results: Vec<SampleResult>,
    pub metrics: ThresholdMetrics,
    #[serde(default)]
    pub per_branch: HashMap<String, ThresholdMetrics>,
}

/// Result of running all branches on a single sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleResult {
    pub sample_id: String,
    pub label: String,
    pub is_positive: bool,
    pub overall_score: f64,
    #[serde(default)]
    pub branch_scores: HashMap<String, f64>,
    #[serde(default)]
    pub processing_ms: u64,
}

/// Generate a human-readable markdown report.
pub fn markdown_report(result: &RunResult) -> String {
    let mut lines = Vec::new();
    let m = &result.metrics;

    let n_pos = result
        .sample_results
        .iter()
        .filter(|s| s.is_positive)
        .count();
    let n_neg = result.sample_results.len() - n_pos;

    lines.push("═".repeat(65));
    lines.push(format!("  BENCH RUN: {}", result.run_id));
    let tag = if result.tag.is_empty() {
        String::new()
    } else {
        format!(" | Tag: {}", result.tag)
    };
    lines.push(format!(
        "  Samples: {} ({} positive, {} negative) | Threshold: {:.2}{}",
        result.sample_results.len(),
        n_pos,
        n_neg,
        m.threshold,
        tag
    ));
    lines.push("═".repeat(65));
    lines.push(String::new());

    lines.push("OVERALL ACCURACY".to_string());
    lines.push(format!(
        "  Precision: {:.3}    Recall: {:.3}    F1: {:.3}    Accuracy: {:.3}",
        m.precision, m.recall, m.f1, m.accuracy
    ));
    lines.push(String::new());

    let cm = &m.confusion;
    lines.push(format!("CONFUSION MATRIX (threshold={:.02})", m.threshold));
    lines.push(format!("{:17}Predicted Positive    Predicted Negative", ""));
    lines.push(format!(
        "  Actual Positive     {:>4} (TP)            {:>4} (FN)",
        cm.tp, cm.fn_count
    ));
    lines.push(format!(
        "  Actual Negative     {:>4} (FP)            {:>4} (TN)",
        cm.fp, cm.tn
    ));
    lines.push(String::new());

    if !result.per_branch.is_empty() {
        lines.push("PER-BRANCH BREAKDOWN".to_string());
        lines.push(format!(
            "  {:<18}{:>6}    {:>13}   {:>13}   {:>10}",
            "Branch", "F1", "Avg Pos Score", "Avg Neg Score", "Separation"
        ));
        lines.push(format!("  {}", "─".repeat(72)));

        let mut branches: Vec<_> = result.per_branch.iter().collect();
        branches.sort_by(|a, b| a.0.cmp(b.0));

        for (name, bm) in &branches {
            let pos_scores: Vec<f64> = result
                .sample_results
                .iter()
                .filter(|s| s.is_positive)
                .filter_map(|s| s.branch_scores.get(*name).copied())
                .collect();
            let neg_scores: Vec<f64> = result
                .sample_results
                .iter()
                .filter(|s| !s.is_positive)
                .filter_map(|s| s.branch_scores.get(*name).copied())
                .collect();

            let avg_pos = if pos_scores.is_empty() {
                0.0
            } else {
                pos_scores.iter().sum::<f64>() / pos_scores.len() as f64
            };
            let avg_neg = if neg_scores.is_empty() {
                0.0
            } else {
                neg_scores.iter().sum::<f64>() / neg_scores.len() as f64
            };
            let sep = avg_pos - avg_neg;

            lines.push(format!(
                "  {:<18}{:>6.3}    {:>13.3}   {:>13.3}   {:>10.3}",
                name, bm.f1, avg_pos, avg_neg, sep
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Generate a machine-readable JSON report.
pub fn json_report(result: &RunResult) -> serde_json::Value {
    serde_json::to_value(result).unwrap_or_default()
}

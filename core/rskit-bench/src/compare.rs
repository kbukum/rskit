//! Run comparison and regression detection.

use crate::result::{BenchRunResult, MetricResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Compares two benchmark runs and classifies metric changes by significance.
pub struct RunComparator {
    threshold: f64,
}

impl RunComparator {
    /// Creates a comparator with the default absolute significance threshold.
    pub fn new() -> Self {
        Self { threshold: 0.01 }
    }

    #[must_use]
    /// Sets the minimum absolute metric delta treated as significant.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Builds a diff from `base` to `target`, including metric changes and per-sample correctness changes.
    pub fn compare(&self, base: &BenchRunResult, target: &BenchRunResult) -> RunDiff {
        let mut changes = Vec::new();

        let base_metrics: HashMap<&str, &MetricResult> =
            base.metrics.iter().map(|m| (m.name.as_str(), m)).collect();

        for tm in &target.metrics {
            if let Some(bm) = base_metrics.get(tm.name.as_str()) {
                let delta = tm.value - bm.value;
                let significant = delta.abs() >= self.threshold;
                changes.push(MetricChange {
                    name: tm.name.clone(),
                    old_value: bm.value,
                    new_value: tm.value,
                    delta,
                    improved: delta > 0.0,
                    significant,
                });

                for (key, &new_val) in &tm.values {
                    if let Some(&old_val) = bm.values.get(key) {
                        let d = new_val - old_val;
                        if d.abs() >= self.threshold {
                            changes.push(MetricChange {
                                name: format!("{}.{}", tm.name, key),
                                old_value: old_val,
                                new_value: new_val,
                                delta: d,
                                improved: d > 0.0,
                                significant: d.abs() >= self.threshold,
                            });
                        }
                    }
                }
            }
        }

        let base_correct: HashMap<&str, bool> = base
            .samples
            .iter()
            .map(|s| (s.id.as_str(), s.correct))
            .collect();
        let mut fixed = Vec::new();
        let mut regressed = Vec::new();
        for ts in &target.samples {
            if let Some(&was_correct) = base_correct.get(ts.id.as_str()) {
                if !was_correct && ts.correct {
                    fixed.push(ts.id.clone());
                }
                if was_correct && !ts.correct {
                    regressed.push(ts.id.clone());
                }
            }
        }

        RunDiff {
            base_id: base.id.clone(),
            target_id: target.id.clone(),
            changes,
            fixed,
            regressed,
        }
    }
}

impl Default for RunComparator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Difference summary between two benchmark runs.
pub struct RunDiff {
    /// Identifier of the baseline run.
    pub base_id: String,
    /// Identifier of the target run being compared.
    pub target_id: String,
    /// Metric-level changes found in metrics present in both runs.
    pub changes: Vec<MetricChange>,
    /// Sample identifiers that were incorrect in the baseline and correct in the target run.
    pub fixed: Vec<String>,
    /// Sample identifiers that were correct in the baseline and incorrect in the target run.
    pub regressed: Vec<String>,
}

impl RunDiff {
    /// Formats the diff as a concise multi-line human-readable summary.
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Comparison: {} → {}", self.base_id, self.target_id));
        for c in &self.changes {
            let icon = if c.improved { "✅" } else { "⚠️" };
            let sign = if c.delta >= 0.0 { "+" } else { "" };
            lines.push(format!(
                "  {} {}: {:.4} → {:.4} ({}{:.4})",
                icon, c.name, c.old_value, c.new_value, sign, c.delta
            ));
        }
        if !self.fixed.is_empty() {
            lines.push(format!("  Fixed: {} samples", self.fixed.len()));
        }
        if !self.regressed.is_empty() {
            lines.push(format!("  Regressed: {} samples", self.regressed.len()));
        }
        lines.join("\n")
    }

    /// Returns true when any significant metric change moved in the negative direction.
    pub fn has_regression(&self) -> bool {
        self.changes.iter().any(|c| !c.improved && c.significant)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A single metric value change between two benchmark runs.
pub struct MetricChange {
    /// Metric name, or dotted sub-metric name for entries from the metric value map.
    pub name: String,
    /// Metric value in the baseline run.
    pub old_value: f64,
    /// Metric value in the target run.
    pub new_value: f64,
    /// Difference computed as `new_value - old_value`.
    pub delta: f64,
    /// Whether the metric value increased in the target run.
    pub improved: bool,
    /// Whether the absolute delta meets the comparator significance threshold.
    pub significant: bool,
}

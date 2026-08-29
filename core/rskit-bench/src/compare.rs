//! Run comparison and regression detection.

use crate::result::{BenchRunResult, MetricDirection, MetricResult};
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
                    direction: tm.direction,
                    improved: tm.direction.is_improvement(delta),
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
                                direction: tm.direction,
                                improved: tm.direction.is_improvement(d),
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
            let icon = match c.direction {
                MetricDirection::Neutral => "≈",
                _ if c.improved => "✅",
                _ => "⚠️",
            };
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

    /// Returns true when any significant metric change moved in its metric's
    /// worse direction. Descriptive ([`MetricDirection::Neutral`]) changes never
    /// count as regressions.
    pub fn has_regression(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.significant && c.direction.is_regression(c.delta))
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
    /// Optimization direction of the metric, inherited by dotted sub-metric entries.
    #[serde(default)]
    pub direction: MetricDirection,
    /// Whether the change moved in the metric's better direction. Always false
    /// for a [`MetricDirection::Neutral`] metric.
    pub improved: bool,
    /// Whether the absolute delta meets the comparator significance threshold.
    pub significant: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::MetricResult;

    fn run_with(id: &str, name: &str, value: f64, direction: MetricDirection) -> BenchRunResult {
        BenchRunResult {
            id: id.to_owned(),
            metrics: vec![MetricResult {
                name: name.to_owned(),
                value,
                direction,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn lower_is_better_decrease_is_improvement_and_not_regression() {
        let base = run_with("base", "mae", 0.30, MetricDirection::LowerIsBetter);
        let target = run_with("target", "mae", 0.20, MetricDirection::LowerIsBetter);
        let diff = RunComparator::new().compare(&base, &target);

        let change = &diff.changes[0];
        assert!(change.delta < 0.0);
        assert!(
            change.improved,
            "a decrease in a lower-is-better metric improves"
        );
        assert!(!diff.has_regression());
    }

    #[test]
    fn lower_is_better_increase_is_regression() {
        let base = run_with("base", "mae", 0.20, MetricDirection::LowerIsBetter);
        let target = run_with("target", "mae", 0.30, MetricDirection::LowerIsBetter);
        let diff = RunComparator::new().compare(&base, &target);

        let change = &diff.changes[0];
        assert!(change.delta > 0.0);
        assert!(
            !change.improved,
            "an increase in a lower-is-better metric is not an improvement"
        );
        assert!(diff.has_regression());
    }

    #[test]
    fn higher_is_better_still_classifies_by_increase() {
        let base = run_with("base", "accuracy", 0.80, MetricDirection::HigherIsBetter);
        let up = run_with("target", "accuracy", 0.90, MetricDirection::HigherIsBetter);
        let down = run_with("target", "accuracy", 0.70, MetricDirection::HigherIsBetter);

        assert!(RunComparator::new().compare(&base, &up).changes[0].improved);
        assert!(!RunComparator::new().compare(&base, &down).changes[0].improved);
        assert!(RunComparator::new().compare(&base, &down).has_regression());
    }

    #[test]
    fn neutral_metric_never_improves_or_regresses() {
        let base = run_with("base", "token_stats", 1000.0, MetricDirection::Neutral);
        let more = run_with("target", "token_stats", 2000.0, MetricDirection::Neutral);
        let less = run_with("target", "token_stats", 500.0, MetricDirection::Neutral);

        let up = RunComparator::new().compare(&base, &more);
        let dn = RunComparator::new().compare(&base, &less);
        assert!(!up.changes[0].improved);
        assert!(!dn.changes[0].improved);
        assert!(!up.has_regression());
        assert!(!dn.has_regression());
    }
}

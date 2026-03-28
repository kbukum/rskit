//! Run comparison and regression detection.

use crate::result::{BenchRunResult, MetricResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct RunComparator {
    threshold: f64,
}

impl RunComparator {
    pub fn new() -> Self {
        Self { threshold: 0.01 }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

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
pub struct RunDiff {
    pub base_id: String,
    pub target_id: String,
    pub changes: Vec<MetricChange>,
    pub fixed: Vec<String>,
    pub regressed: Vec<String>,
}

impl RunDiff {
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

    pub fn has_regression(&self) -> bool {
        self.changes.iter().any(|c| !c.improved && c.significant)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricChange {
    pub name: String,
    pub old_value: f64,
    pub new_value: f64,
    pub delta: f64,
    pub improved: bool,
    pub significant: bool,
}

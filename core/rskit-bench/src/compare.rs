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
                            // Each subvalue resolves its own direction: a heterogeneous
                            // metric overrides the inherited direction per key.
                            let key_dir = tm.directions.get(key).copied().unwrap_or(tm.direction);
                            changes.push(MetricChange {
                                name: format!("{}.{}", tm.name, key),
                                old_value: old_val,
                                new_value: new_val,
                                delta: d,
                                direction: key_dir,
                                improved: key_dir.is_improvement(d),
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

        let mut incompatible = Vec::new();
        let base_judges: std::collections::HashMap<&str, &crate::provenance::JudgeProvenance> =
            base.provenance
                .judges
                .iter()
                .map(|judge| (judge.metric.as_str(), judge))
                .collect();
        for target_judge in &target.provenance.judges {
            if let Some(base_judge) = base_judges.get(target_judge.metric.as_str())
                && base_judge.resolved_model != target_judge.resolved_model
            {
                incompatible.push(JudgeIncompatibility {
                    metric: target_judge.metric.clone(),
                    base_resolved_model: base_judge.resolved_model.clone(),
                    target_resolved_model: target_judge.resolved_model.clone(),
                });
            }
        }

        RunDiff {
            base_id: base.id.clone(),
            target_id: target.id.clone(),
            changes,
            fixed,
            regressed,
            incompatible,
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
    /// Judge metrics whose provider resolved the requested model to a different backend model
    /// between the two runs, so their scores are not directly comparable.
    ///
    /// These metrics still appear in [`changes`](Self::changes) for display, but they and their
    /// `"<metric>.<key>"` subvalues are excluded from [`has_regression`](Self::has_regression):
    /// a delta between scores produced by different backend models is not a like-for-like
    /// comparison and must never trip an automated regression gate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incompatible: Vec<JudgeIncompatibility>,
}

/// A judge metric that resolved to different backend models across two compared runs.
///
/// Recorded when the same judge metric is present in both runs' provenance but the provider
/// reported a different [`resolved_model`](crate::provenance::JudgeProvenance::resolved_model)
/// (including one run reporting a model and the other reporting none). Its score delta stays in
/// the diff for display but is excluded from regression detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeIncompatibility {
    /// Judge metric name, the join key shared by both runs' provenance.
    pub metric: String,
    /// Backend model the baseline run's provider resolved the judge to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_resolved_model: Option<String>,
    /// Backend model the target run's provider resolved the judge to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_resolved_model: Option<String>,
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
                _ if c.direction.is_regression(c.delta) => "⚠️",
                _ => "➖",
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
        for inc in &self.incompatible {
            lines.push(format!(
                "  ⚠️ {}: judges differ; scores are not directly comparable",
                inc.metric
            ));
        }
        lines.join("\n")
    }

    /// Returns true when any significant metric change moved in its metric's
    /// worse direction. Descriptive ([`MetricDirection::Neutral`]) changes never
    /// count as regressions.
    ///
    /// Changes belonging to an [`incompatible`](Self::incompatible) judge metric — the metric
    /// itself or any of its `"<metric>.<key>"` subvalues — are excluded: a delta between scores
    /// produced by different backend models is not a like-for-like comparison, so it must never
    /// count as a regression.
    pub fn has_regression(&self) -> bool {
        self.changes.iter().any(|c| {
            c.significant
                && c.direction.is_regression(c.delta)
                && !self.is_incompatible_change(&c.name)
        })
    }

    /// Whether a change name belongs to an incompatible judge metric — either the metric name
    /// itself or one of its `"<metric>.<key>"` subvalues. The prefix is anchored on the full
    /// metric name (judge names themselves contain dots) followed by a `.`.
    fn is_incompatible_change(&self, name: &str) -> bool {
        self.incompatible.iter().any(|inc| {
            name == inc.metric
                || name
                    .strip_prefix(&inc.metric)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
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
    /// Optimization direction resolved for this change: the metric's top-level
    /// direction, or its per-key override for a dotted sub-metric entry.
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
    use crate::provenance::{JudgeProvenance, RunProvenance};
    use crate::result::MetricResult;
    use std::collections::{BTreeMap, HashMap};

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

    fn run_with_subvalue(
        id: &str,
        value: f64,
        key: &str,
        sub: f64,
        dir: MetricDirection,
    ) -> BenchRunResult {
        let mut values = HashMap::new();
        values.insert(key.to_owned(), sub);
        let mut directions = HashMap::new();
        directions.insert(key.to_owned(), dir);
        BenchRunResult {
            id: id.to_owned(),
            metrics: vec![MetricResult {
                name: "classification".to_owned(),
                value,
                direction: MetricDirection::HigherIsBetter,
                directions,
                values,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn subvalue_direction_override_classifies_per_key() {
        // A lower-is-better false-positive rate that rises under a higher-is-better
        // metric must be flagged as a regression, not inherited as an improvement.
        let base = run_with_subvalue("base", 0.80, "fpr", 0.10, MetricDirection::LowerIsBetter);
        let target = run_with_subvalue("target", 0.80, "fpr", 0.30, MetricDirection::LowerIsBetter);
        let diff = RunComparator::new().compare(&base, &target);

        let fpr = diff
            .changes
            .iter()
            .find(|c| c.name == "classification.fpr")
            .expect("missing classification.fpr change");
        assert_eq!(fpr.direction, MetricDirection::LowerIsBetter);
        assert!(
            !fpr.improved,
            "a rising false-positive rate is not an improvement"
        );
        assert!(diff.has_regression());
    }

    #[test]
    fn summary_marks_unchanged_directional_metric_as_neither() {
        let base = run_with("base", "f1", 0.80, MetricDirection::HigherIsBetter);
        let target = run_with("target", "f1", 0.80, MetricDirection::HigherIsBetter);
        let summary = RunComparator::new().compare(&base, &target).summary();
        assert!(
            !summary.contains("⚠️"),
            "an unchanged metric must not render a warning: {summary}"
        );
        assert!(
            !summary.contains("✅"),
            "an unchanged metric must not render an improvement: {summary}"
        );
    }

    /// Builds a run carrying one judge metric plus, optionally, a `"<metric>.<key>"` subvalue,
    /// and a matching judge-provenance entry recording the resolved backend model.
    fn judge_run(
        id: &str,
        metric: &str,
        value: f64,
        resolved_model: Option<&str>,
        subvalue: Option<(&str, f64)>,
    ) -> BenchRunResult {
        let mut values = BTreeMap::new();
        if let Some((key, v)) = subvalue {
            values.insert(key.to_owned(), v);
        }
        let mut provenance_judge = JudgeProvenance::new(
            metric,
            "openai",
            "gpt-judge",
            "rskit.builtin.judge",
            "1.0.0",
            "fingerprint",
        );
        if let Some(model) = resolved_model {
            provenance_judge = provenance_judge.with_resolved_model(model);
        }
        let judges = vec![provenance_judge];
        BenchRunResult {
            id: id.to_owned(),
            metrics: vec![MetricResult {
                name: metric.to_owned(),
                value,
                direction: MetricDirection::HigherIsBetter,
                values: values.into_iter().collect(),
                ..Default::default()
            }],
            provenance: RunProvenance {
                judges,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    const JUDGE: &str = "llm_judge[openai/gpt-judge@rskit.builtin.judge@1.0.0:t0.5]";

    #[test]
    fn differing_resolved_models_are_flagged_incompatible() {
        let base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let target = judge_run("target", JUDGE, 0.90, Some("gpt-4o-2024-08-06"), None);
        let diff = RunComparator::new().compare(&base, &target);

        assert_eq!(diff.incompatible.len(), 1);
        let inc = &diff.incompatible[0];
        assert_eq!(inc.metric, JUDGE);
        assert_eq!(inc.base_resolved_model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(
            inc.target_resolved_model.as_deref(),
            Some("gpt-4o-2024-08-06")
        );
    }

    #[test]
    fn some_versus_none_resolution_is_incompatible() {
        let base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let target = judge_run("target", JUDGE, 0.90, None, None);
        let diff = RunComparator::new().compare(&base, &target);

        assert_eq!(diff.incompatible.len(), 1);
        assert_eq!(
            diff.incompatible[0].base_resolved_model.as_deref(),
            Some("gpt-4o-mini")
        );
        assert_eq!(diff.incompatible[0].target_resolved_model, None);
    }

    #[test]
    fn identical_resolved_model_is_not_flagged() {
        let base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let target = judge_run("target", JUDGE, 0.70, Some("gpt-4o-mini"), None);
        let diff = RunComparator::new().compare(&base, &target);

        assert!(diff.incompatible.is_empty());
    }

    #[test]
    fn incompatible_judge_regression_is_excluded() {
        // The judge score drops sharply, but because the two runs resolved to different backend
        // models the delta is not like-for-like and must not trip the regression gate.
        let base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let target = judge_run("target", JUDGE, 0.40, Some("gpt-4o-2024-08-06"), None);
        let diff = RunComparator::new().compare(&base, &target);

        assert!(!diff.incompatible.is_empty());
        assert!(!diff.has_regression());
    }

    #[test]
    fn compatible_metric_still_regresses_alongside_an_incompatible_judge() {
        let mut base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let mut target = judge_run("target", JUDGE, 0.40, Some("gpt-4o-2024-08-06"), None);
        base.metrics.push(MetricResult {
            name: "accuracy".to_owned(),
            value: 0.90,
            direction: MetricDirection::HigherIsBetter,
            ..Default::default()
        });
        target.metrics.push(MetricResult {
            name: "accuracy".to_owned(),
            value: 0.70,
            direction: MetricDirection::HigherIsBetter,
            ..Default::default()
        });
        let diff = RunComparator::new().compare(&base, &target);

        assert!(
            diff.has_regression(),
            "a genuinely regressing compatible metric still counts"
        );
    }

    #[test]
    fn incompatible_judge_subvalue_regression_is_excluded() {
        let base = judge_run(
            "base",
            JUDGE,
            0.90,
            Some("gpt-4o-mini"),
            Some(("pass_rate", 0.90)),
        );
        let target = judge_run(
            "target",
            JUDGE,
            0.90,
            Some("gpt-4o-2024-08-06"),
            Some(("pass_rate", 0.40)),
        );
        let diff = RunComparator::new().compare(&base, &target);

        assert!(
            diff.changes
                .iter()
                .any(|c| c.name == format!("{JUDGE}.pass_rate")),
            "the subvalue delta is retained for display"
        );
        assert!(
            !diff.has_regression(),
            "the '<metric>.pass_rate' subvalue is excluded with its incompatible parent"
        );
    }

    #[test]
    fn incompatible_metric_change_is_retained_and_summarized() {
        let base = judge_run("base", JUDGE, 0.90, Some("gpt-4o-mini"), None);
        let target = judge_run("target", JUDGE, 0.40, Some("gpt-4o-2024-08-06"), None);
        let diff = RunComparator::new().compare(&base, &target);

        assert!(
            diff.changes.iter().any(|c| c.name == JUDGE),
            "the incompatible metric delta stays in changes for display"
        );
        assert!(
            diff.summary().contains("judges differ"),
            "the summary reports the incompatibility"
        );
    }

    #[test]
    fn empty_incompatible_is_omitted_from_json() {
        let base = run_with("base", "mae", 0.30, MetricDirection::LowerIsBetter);
        let target = run_with("target", "mae", 0.20, MetricDirection::LowerIsBetter);
        let diff = RunComparator::new().compare(&base, &target);
        let json = serde_json::to_string(&diff).expect("serialize");
        assert!(
            !json.contains("incompatible"),
            "an empty incompatibility list is not serialized"
        );
    }
}

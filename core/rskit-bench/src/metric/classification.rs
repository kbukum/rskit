use super::Metric;
use super::identity::format_threshold;
use crate::curves::{ConfusionMatrixDetail, ThresholdPoint};
use crate::{MetricDirection, MetricResult, ScoredSample};
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashMap;
use std::fmt::Display;

fn safe_divide(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

/// Creates a binary classification metric that reports precision, recall, F1, accuracy, false-positive rate, and confusion detail.
///
/// The configured threshold is folded into the metric name (for example `classification[t0.5]`) and recorded in [`MetricResult::detail`] provenance rather than [`MetricResult::values`]: it is a configuration input, not a quality signal. Because `f1` (the primary value) and every confusion-derived value are computed at this cutoff, embedding the threshold in the identity keeps runs scored at different thresholds distinct under [`RunComparator`](crate::compare::RunComparator) instead of joining them and scoring an incomparable delta as an improvement or regression. The threshold must be finite and in `[0, 1]`; an invalid value is rejected as a typed [`ErrorCode::InvalidInput`] error when the metric is computed, rather than yielding a `tNaN` identity and a `null` provenance value.
pub fn binary_classification<L>(positive_label: L, threshold: f64) -> Box<dyn Metric<L>>
where
    L: PartialEq + Display + Clone + Send + Sync + 'static,
{
    Box::new(BinaryClassification {
        positive: positive_label,
        threshold,
        name: build_name(threshold),
    })
}

/// Builds the comparison-safe metric name from the configured threshold.
///
/// The threshold is part of the identity because `f1` (the primary value) and every confusion-derived value are computed at this cutoff: two runs scored at different thresholds must never join under a shared name, or [`RunComparator`](crate::compare::RunComparator) would score an incomparable delta as a regression or improvement.
fn build_name(threshold: f64) -> String {
    format!("classification[t{}]", format_threshold(threshold))
}

struct BinaryClassification<L> {
    positive: L,
    threshold: f64,
    name: String,
}

impl<L: PartialEq + Display + Clone + Send + Sync + 'static> Metric<L> for BinaryClassification<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        if !self.threshold.is_finite() || !(0.0..=1.0).contains(&self.threshold) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "classification: threshold {} is out of the required range [0, 1]",
                    self.threshold
                ),
            ));
        }
        let (mut tp, mut fp, mut tn, mut fn_) = (0i64, 0i64, 0i64, 0i64);
        for s in scored {
            let actual = s.sample.label == self.positive;
            let predicted = s.prediction.score >= self.threshold;
            match (actual, predicted) {
                (true, true) => tp += 1,
                (false, true) => fp += 1,
                (true, false) => fn_ += 1,
                (false, false) => tn += 1,
            }
        }
        let precision = safe_divide(tp as f64, (tp + fp) as f64);
        let recall = safe_divide(tp as f64, (tp + fn_) as f64);
        let f1 = safe_divide(2.0 * precision * recall, precision + recall);
        let accuracy = safe_divide((tp + tn) as f64, scored.len() as f64);
        let fpr = safe_divide(fp as f64, (fp + tn) as f64);

        let mut neg_label = String::new();
        for s in scored {
            if s.sample.label != self.positive {
                neg_label = format!("{}", s.sample.label);
                break;
            }
        }

        let mut values = HashMap::new();
        values.insert("precision".into(), precision);
        values.insert("recall".into(), recall);
        values.insert("f1".into(), f1);
        values.insert("accuracy".into(), accuracy);
        values.insert("fpr".into(), fpr);
        values.insert("tp".into(), tp as f64);
        values.insert("fp".into(), fp as f64);
        values.insert("tn".into(), tn as f64);
        values.insert("fn".into(), fn_ as f64);

        // f1/precision/recall/accuracy inherit the metric's higher-is-better
        // direction; the confusion-derived diagnostics have their own: a lower
        // false-positive rate (and fewer false positives/negatives) is better,
        // while the raw tp/tn counts are descriptive.
        let mut directions = HashMap::new();
        directions.insert("fpr".into(), MetricDirection::LowerIsBetter);
        directions.insert("fp".into(), MetricDirection::LowerIsBetter);
        directions.insert("fn".into(), MetricDirection::LowerIsBetter);
        directions.insert("tp".into(), MetricDirection::Neutral);
        directions.insert("tn".into(), MetricDirection::Neutral);

        let detail = ConfusionMatrixDetail {
            labels: vec![format!("{}", self.positive), neg_label],
            matrix: vec![vec![tp, fn_], vec![fp, tn]],
            orientation: "row=actual, col=predicted".into(),
        };
        // The threshold is a configuration input, not a quality signal, so it lives in provenance detail rather than `values`, where `RunComparator` would score a threshold change as an improvement or regression.
        let mut detail = serde_json::to_value(&detail).unwrap_or_default();
        if let Some(obj) = detail.as_object_mut() {
            obj.insert("threshold".into(), serde_json::json!(self.threshold));
        }

        Ok(MetricResult {
            name: self.name.clone(),
            value: f1,
            direction: MetricDirection::HigherIsBetter,
            directions,
            values,
            detail: Some(detail),
        })
    }
}

/// Creates a metric that returns an N×N confusion matrix for the supplied label order.
pub fn confusion_matrix<L>(labels: Vec<L>) -> Box<dyn Metric<L>>
where
    L: PartialEq + Display + Clone + Send + Sync + 'static,
{
    Box::new(ConfusionMatrixMetric { labels })
}

struct ConfusionMatrixMetric<L> {
    labels: Vec<L>,
}

impl<L: PartialEq + Display + Clone + Send + Sync + 'static> Metric<L>
    for ConfusionMatrixMetric<L>
{
    fn name(&self) -> &str {
        "confusion_matrix"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        let n = self.labels.len();
        let mut matrix = vec![vec![0i64; n]; n];
        for s in scored {
            let actual = self.labels.iter().position(|l| *l == s.sample.label);
            let predicted = self.labels.iter().position(|l| *l == s.prediction.label);
            if let (Some(a), Some(p)) = (actual, predicted) {
                matrix[a][p] += 1;
            }
        }
        let label_strings: Vec<String> = self.labels.iter().map(|l| format!("{l}")).collect();
        let detail = ConfusionMatrixDetail {
            labels: label_strings,
            matrix,
            orientation: "row=actual, col=predicted".into(),
        };
        Ok(MetricResult {
            directions: Default::default(),
            name: "confusion_matrix".into(),
            value: 0.0,
            direction: MetricDirection::Neutral,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&detail).unwrap_or_default()),
        })
    }
}

/// Creates a metric that evaluates binary classification quality across thresholds and reports the best F1 score.
pub fn threshold_sweep<L>(positive_label: L, thresholds: Option<Vec<f64>>) -> Box<dyn Metric<L>>
where
    L: PartialEq + Display + Clone + Send + Sync + 'static,
{
    let t = thresholds.unwrap_or_else(|| vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    Box::new(ThresholdSweepMetric {
        positive: positive_label,
        thresholds: t,
    })
}

struct ThresholdSweepMetric<L> {
    positive: L,
    thresholds: Vec<f64>,
}

impl<L: PartialEq + Display + Clone + Send + Sync + 'static> Metric<L> for ThresholdSweepMetric<L> {
    fn name(&self) -> &str {
        "threshold_sweep"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        let mut best_f1 = 0.0f64;
        let mut points = Vec::new();
        for &t in &self.thresholds {
            let (mut tp, mut fp, mut tn, mut fn_) = (0i64, 0i64, 0i64, 0i64);
            for s in scored {
                let actual = s.sample.label == self.positive;
                let predicted = s.prediction.score >= t;
                match (actual, predicted) {
                    (true, true) => tp += 1,
                    (false, true) => fp += 1,
                    (true, false) => fn_ += 1,
                    (false, false) => tn += 1,
                }
            }
            let prec = safe_divide(tp as f64, (tp + fp) as f64);
            let rec = safe_divide(tp as f64, (tp + fn_) as f64);
            let f1 = safe_divide(2.0 * prec * rec, prec + rec);
            let acc = safe_divide((tp + tn) as f64, scored.len() as f64);
            if f1 > best_f1 {
                best_f1 = f1;
            }
            points.push(ThresholdPoint {
                threshold: t,
                precision: prec,
                recall: rec,
                f1,
                accuracy: acc,
            });
        }
        Ok(MetricResult {
            directions: Default::default(),
            name: "threshold_sweep".into(),
            value: best_f1,
            direction: MetricDirection::HigherIsBetter,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&points).unwrap_or_default()),
        })
    }
}

/// Creates a multi-class classification metric with macro and micro precision, recall, F1, and accuracy values.
pub fn multi_class_classification<L>(labels: Vec<L>) -> Box<dyn Metric<L>>
where
    L: PartialEq + Display + Clone + Send + Sync + 'static,
{
    Box::new(MultiClassMetric { labels })
}

struct MultiClassMetric<L> {
    labels: Vec<L>,
}

impl<L: PartialEq + Display + Clone + Send + Sync + 'static> Metric<L> for MultiClassMetric<L> {
    fn name(&self) -> &str {
        "multi_class_classification"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        let n = self.labels.len();
        let mut tp = vec![0usize; n];
        let mut fp = vec![0usize; n];
        let mut fn_ = vec![0usize; n];

        for s in scored {
            let actual = self.labels.iter().position(|l| *l == s.sample.label);
            let predicted = self.labels.iter().position(|l| *l == s.prediction.label);
            match (actual, predicted) {
                (Some(a), Some(p)) if a == p => tp[a] += 1,
                (Some(a), Some(p)) => {
                    fn_[a] += 1;
                    fp[p] += 1;
                }
                _ => {}
            }
        }

        let (mut macro_p, mut macro_r, mut macro_f1) = (0.0, 0.0, 0.0);
        let mut count = 0;
        for i in 0..n {
            if tp[i] + fp[i] + fn_[i] > 0 {
                let p = safe_divide(tp[i] as f64, (tp[i] + fp[i]) as f64);
                let r = safe_divide(tp[i] as f64, (tp[i] + fn_[i]) as f64);
                let f = safe_divide(2.0 * p * r, p + r);
                macro_p += p;
                macro_r += r;
                macro_f1 += f;
                count += 1;
            }
        }
        if count > 0 {
            macro_p /= count as f64;
            macro_r /= count as f64;
            macro_f1 /= count as f64;
        }

        let total_tp: usize = tp.iter().sum();
        let total_fp: usize = fp.iter().sum();
        let total_fn: usize = fn_.iter().sum();
        let micro_p = safe_divide(total_tp as f64, (total_tp + total_fp) as f64);
        let micro_r = safe_divide(total_tp as f64, (total_tp + total_fn) as f64);
        let micro_f1 = safe_divide(2.0 * micro_p * micro_r, micro_p + micro_r);

        let correct = scored
            .iter()
            .filter(|s| s.sample.label == s.prediction.label)
            .count();
        let accuracy = safe_divide(correct as f64, scored.len() as f64);

        let mut values = HashMap::new();
        values.insert("macro_precision".into(), macro_p);
        values.insert("macro_recall".into(), macro_r);
        values.insert("macro_f1".into(), macro_f1);
        values.insert("micro_precision".into(), micro_p);
        values.insert("micro_recall".into(), micro_r);
        values.insert("micro_f1".into(), micro_f1);
        values.insert("accuracy".into(), accuracy);

        Ok(MetricResult {
            directions: Default::default(),
            name: "multi_class_classification".into(),
            value: macro_f1,
            direction: MetricDirection::HigherIsBetter,
            values,
            detail: None,
        })
    }
}

#[cfg(test)]
mod direction_tests {
    use super::*;
    use crate::types::{BenchSample, Prediction, ScoredSample};
    use std::collections::HashMap;

    fn scored(label: &str, pred: &str, score: f64) -> ScoredSample<String> {
        ScoredSample {
            sample: BenchSample {
                id: "s".into(),
                input: vec![],
                label: label.into(),
                source: String::new(),
                metadata: HashMap::new(),
            },
            prediction: Prediction {
                sample_id: "s".into(),
                label: pred.into(),
                score,
                scores: HashMap::new(),
                metadata: HashMap::new(),
            },
        }
    }

    #[test]
    fn binary_classification_subvalue_directions() {
        let m = binary_classification::<String>("pos".into(), 0.5);
        let r = m
            .compute(&[scored("pos", "pos", 0.9), scored("neg", "neg", 0.2)])
            .unwrap();
        assert_eq!(r.direction, MetricDirection::HigherIsBetter);
        for k in ["fpr", "fp", "fn"] {
            assert_eq!(
                r.directions.get(k),
                Some(&MetricDirection::LowerIsBetter),
                "{k}"
            );
        }
        for k in ["tp", "tn"] {
            assert_eq!(r.directions.get(k), Some(&MetricDirection::Neutral), "{k}");
        }
        // Headline values inherit the top-level direction (absent from directions).
        for k in ["f1", "precision", "recall", "accuracy"] {
            assert!(!r.directions.contains_key(k), "{k} should inherit");
        }
    }
}

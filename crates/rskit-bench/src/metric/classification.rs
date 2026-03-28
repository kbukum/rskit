use super::{Metric, MetricResult, ScoredSample};
use crate::curves::{ConfusionMatrixDetail, ThresholdPoint};
use std::collections::HashMap;
use std::fmt::Display;

fn safe_divide(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

pub fn binary_classification<L>(positive_label: L, threshold: f64) -> Box<dyn Metric<L>>
where
    L: PartialEq + Display + Clone + Send + Sync + 'static,
{
    Box::new(BinaryClassification {
        positive: positive_label,
        threshold,
    })
}

struct BinaryClassification<L> {
    positive: L,
    threshold: f64,
}

impl<L: PartialEq + Display + Clone + Send + Sync + 'static> Metric<L> for BinaryClassification<L> {
    fn name(&self) -> &str {
        "classification"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
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
        values.insert("threshold".into(), self.threshold);

        let detail = ConfusionMatrixDetail {
            labels: vec![format!("{}", self.positive), neg_label],
            matrix: vec![vec![tp, fn_], vec![fp, tn]],
            orientation: "row=actual, col=predicted".into(),
        };

        MetricResult {
            name: "classification".into(),
            value: f1,
            values,
            detail: Some(serde_json::to_value(&detail).unwrap_or_default()),
        }
    }
}

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

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        let n = self.labels.len();
        let mut matrix = vec![vec![0i64; n]; n];
        for s in scored {
            let actual = self.labels.iter().position(|l| *l == s.sample.label);
            let predicted = self.labels.iter().position(|l| *l == s.prediction.label);
            if let (Some(a), Some(p)) = (actual, predicted) {
                matrix[a][p] += 1;
            }
        }
        let label_strings: Vec<String> = self.labels.iter().map(|l| format!("{}", l)).collect();
        let detail = ConfusionMatrixDetail {
            labels: label_strings,
            matrix,
            orientation: "row=actual, col=predicted".into(),
        };
        MetricResult {
            name: "confusion_matrix".into(),
            value: 0.0,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&detail).unwrap_or_default()),
        }
    }
}

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

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
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
        MetricResult {
            name: "threshold_sweep".into(),
            value: best_f1,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&points).unwrap_or_default()),
        }
    }
}

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

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
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

        MetricResult {
            name: "multi_class_classification".into(),
            value: macro_f1,
            values,
            detail: None,
        }
    }
}

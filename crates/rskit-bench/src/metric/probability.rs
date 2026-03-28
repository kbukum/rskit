use super::{Metric, MetricResult, ScoredSample};
use crate::curves::{CalibrationCurve, RocCurve};
use std::collections::HashMap;

fn empty_result(name: &str) -> MetricResult {
    MetricResult {
        name: name.into(),
        value: 0.0,
        values: HashMap::new(),
        detail: None,
    }
}

/// AUC-ROC metric for binary classification.
pub fn auc_roc<L>(positive_label: L) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(AucRoc {
        positive: positive_label,
    })
}

struct AucRoc<L> {
    positive: L,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for AucRoc<L> {
    fn name(&self) -> &str {
        "aucroc"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("aucroc");
        }

        let mut sorted: Vec<_> = scored.to_vec();
        sorted.sort_by(|a, b| {
            b.prediction
                .score
                .partial_cmp(&a.prediction.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_pos = sorted
            .iter()
            .filter(|s| s.sample.label == self.positive)
            .count();
        let total_neg = sorted.len() - total_pos;
        if total_pos == 0 || total_neg == 0 {
            return empty_result("aucroc");
        }

        let mut fpr_vec = vec![0.0];
        let mut tpr_vec = vec![0.0];
        let mut thresholds = vec![f64::INFINITY];
        let (mut tp, mut fp) = (0usize, 0usize);

        for s in &sorted {
            if s.sample.label == self.positive {
                tp += 1;
            } else {
                fp += 1;
            }
            fpr_vec.push(fp as f64 / total_neg as f64);
            tpr_vec.push(tp as f64 / total_pos as f64);
            thresholds.push(s.prediction.score);
        }

        let mut auc = 0.0;
        for i in 1..fpr_vec.len() {
            auc += (fpr_vec[i] - fpr_vec[i - 1]) * (tpr_vec[i] + tpr_vec[i - 1]) / 2.0;
        }

        let detail = RocCurve {
            fpr: fpr_vec,
            tpr: tpr_vec,
            thresholds,
            auc,
        };
        MetricResult {
            name: "aucroc".into(),
            value: auc,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&detail).unwrap_or_default()),
        }
    }
}

/// Brier score for probability calibration.
pub fn brier_score<L>(positive_label: L) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(BrierScore {
        positive: positive_label,
    })
}

struct BrierScore<L> {
    positive: L,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for BrierScore<L> {
    fn name(&self) -> &str {
        "brier_score"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("brier_score");
        }
        let sum: f64 = scored
            .iter()
            .map(|s| {
                let actual = if s.sample.label == self.positive {
                    1.0
                } else {
                    0.0
                };
                (s.prediction.score - actual).powi(2)
            })
            .sum();
        MetricResult {
            name: "brier_score".into(),
            value: sum / scored.len() as f64,
            values: HashMap::new(),
            detail: None,
        }
    }
}

/// Log loss (binary cross-entropy).
pub fn log_loss<L>(positive_label: L) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(LogLoss {
        positive: positive_label,
    })
}

struct LogLoss<L> {
    positive: L,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for LogLoss<L> {
    fn name(&self) -> &str {
        "log_loss"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("log_loss");
        }
        let eps = 1e-15;
        let sum: f64 = scored
            .iter()
            .map(|s| {
                let actual = if s.sample.label == self.positive {
                    1.0
                } else {
                    0.0
                };
                let p = s.prediction.score.max(eps).min(1.0 - eps);
                actual * p.ln() + (1.0 - actual) * (1.0 - p).ln()
            })
            .sum();
        MetricResult {
            name: "log_loss".into(),
            value: -sum / scored.len() as f64,
            values: HashMap::new(),
            detail: None,
        }
    }
}

/// Calibration metric: expected calibration error with binned curve.
pub fn calibration<L>(positive_label: L, bins: usize) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(CalibrationMetric {
        positive: positive_label,
        bins: if bins == 0 { 10 } else { bins },
    })
}

struct CalibrationMetric<L> {
    positive: L,
    bins: usize,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for CalibrationMetric<L> {
    fn name(&self) -> &str {
        "calibration"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("calibration");
        }
        let bin_width = 1.0 / self.bins as f64;
        let mut bin_count = vec![0usize; self.bins];
        let mut bin_pos = vec![0usize; self.bins];
        let mut bin_score_sum = vec![0.0f64; self.bins];

        for s in scored {
            let idx = ((s.prediction.score / bin_width) as usize).min(self.bins - 1);
            bin_count[idx] += 1;
            bin_score_sum[idx] += s.prediction.score;
            if s.sample.label == self.positive {
                bin_pos[idx] += 1;
            }
        }

        let mut pred_prob = vec![0.0; self.bins];
        let mut actual_freq = vec![0.0; self.bins];
        let total = scored.len() as f64;
        let mut ece = 0.0;

        for i in 0..self.bins {
            if bin_count[i] > 0 {
                pred_prob[i] = bin_score_sum[i] / bin_count[i] as f64;
                actual_freq[i] = bin_pos[i] as f64 / bin_count[i] as f64;
                ece += (bin_count[i] as f64 / total) * (actual_freq[i] - pred_prob[i]).abs();
            }
        }

        let detail = CalibrationCurve {
            predicted_probability: pred_prob,
            actual_frequency: actual_freq,
            bin_count,
        };
        MetricResult {
            name: "calibration".into(),
            value: ece,
            values: HashMap::new(),
            detail: Some(serde_json::to_value(&detail).unwrap_or_default()),
        }
    }
}

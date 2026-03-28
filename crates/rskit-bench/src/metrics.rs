//! Accuracy metric computation for benchmarking.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfusionMatrix {
    pub tp: u32,
    pub fp: u32,
    pub tn: u32,
    pub fn_count: u32,
}

impl ConfusionMatrix {
    pub fn total(&self) -> u32 {
        self.tp + self.fp + self.tn + self.fn_count
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdMetrics {
    pub threshold: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub accuracy: f64,
    pub fpr: f64,
    pub confusion: ConfusionMatrix,
}

pub fn compute_metrics(scores: &[f64], labels: &[bool], threshold: f64) -> ThresholdMetrics {
    assert_eq!(
        scores.len(),
        labels.len(),
        "scores and labels must have same length"
    );

    let mut cm = ConfusionMatrix::default();

    for (score, label) in scores.iter().zip(labels.iter()) {
        let predicted_positive = *score >= threshold;
        match (*label, predicted_positive) {
            (true, true) => cm.tp += 1,
            (true, false) => cm.fn_count += 1,
            (false, true) => cm.fp += 1,
            (false, false) => cm.tn += 1,
        }
    }

    let precision = if cm.tp + cm.fp > 0 {
        cm.tp as f64 / (cm.tp + cm.fp) as f64
    } else {
        0.0
    };

    let recall = if cm.tp + cm.fn_count > 0 {
        cm.tp as f64 / (cm.tp + cm.fn_count) as f64
    } else {
        0.0
    };

    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    let total = cm.total() as f64;
    let accuracy = if total > 0.0 {
        (cm.tp + cm.tn) as f64 / total
    } else {
        0.0
    };

    let fpr = if cm.fp + cm.tn > 0 {
        cm.fp as f64 / (cm.fp + cm.tn) as f64
    } else {
        0.0
    };

    ThresholdMetrics {
        threshold,
        precision: (precision * 10000.0).round() / 10000.0,
        recall: (recall * 10000.0).round() / 10000.0,
        f1: (f1 * 10000.0).round() / 10000.0,
        accuracy: (accuracy * 10000.0).round() / 10000.0,
        fpr: (fpr * 10000.0).round() / 10000.0,
        confusion: cm,
    }
}

pub fn threshold_sweep(scores: &[f64], labels: &[bool]) -> Vec<ThresholdMetrics> {
    let thresholds: Vec<f64> = (1..10).map(|i| i as f64 * 0.1).collect();
    thresholds
        .iter()
        .map(|t| compute_metrics(scores, labels, *t))
        .collect()
}

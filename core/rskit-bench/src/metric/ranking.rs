use super::Metric;
use crate::{MetricResult, ScoredSample};
use std::collections::HashMap;
use std::marker::PhantomData;

fn safe_divide(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

fn empty_result(name: &str) -> MetricResult {
    MetricResult {
        name: name.into(),
        value: 0.0,
        values: HashMap::new(),
        detail: None,
    }
}

/// Normalized Discounted Cumulative Gain at k.
pub fn ndcg<L>(k: usize) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(NdcgMetric::<L> {
        k,
        _phantom: PhantomData,
    })
}

struct NdcgMetric<L> {
    k: usize,
    _phantom: PhantomData<L>,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for NdcgMetric<L> {
    fn name(&self) -> &str {
        "ndcg"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("ndcg");
        }

        let mut sorted: Vec<_> = scored.to_vec();
        sorted.sort_by(|a, b| {
            b.prediction
                .score
                .partial_cmp(&a.prediction.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let n = if self.k > 0 && self.k < sorted.len() {
            self.k
        } else {
            sorted.len()
        };
        let rels: Vec<f64> = sorted
            .iter()
            .map(|s| {
                if s.sample.label == s.prediction.label {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();

        let dcg: f64 = rels[..n]
            .iter()
            .enumerate()
            .map(|(i, r)| r / (i as f64 + 2.0).log2())
            .sum();

        let mut ideal_rels = rels.clone();
        ideal_rels.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let ideal_dcg: f64 = ideal_rels[..n]
            .iter()
            .enumerate()
            .map(|(i, r)| r / (i as f64 + 2.0).log2())
            .sum();

        let val = safe_divide(dcg, ideal_dcg);
        let mut values = HashMap::new();
        values.insert("dcg".into(), dcg);
        values.insert("ideal_dcg".into(), ideal_dcg);
        values.insert("k".into(), n as f64);

        MetricResult {
            name: "ndcg".into(),
            value: val,
            values,
            detail: None,
        }
    }
}

/// Mean Average Precision.
pub fn mean_average_precision<L>(positive_label: L) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(MeanAvgPrecision {
        positive: positive_label,
    })
}

struct MeanAvgPrecision<L> {
    positive: L,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for MeanAvgPrecision<L> {
    fn name(&self) -> &str {
        "mean_average_precision"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("mean_average_precision");
        }

        let mut sorted: Vec<_> = scored.to_vec();
        sorted.sort_by(|a, b| {
            b.prediction
                .score
                .partial_cmp(&a.prediction.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_relevant = sorted
            .iter()
            .filter(|s| s.sample.label == self.positive)
            .count();
        if total_relevant == 0 {
            return empty_result("mean_average_precision");
        }

        let mut hits = 0usize;
        let mut sum_precision = 0.0;
        for (i, s) in sorted.iter().enumerate() {
            if s.sample.label == self.positive {
                hits += 1;
                sum_precision += hits as f64 / (i + 1) as f64;
            }
        }

        let map = sum_precision / total_relevant as f64;
        MetricResult {
            name: "mean_average_precision".into(),
            value: map,
            values: HashMap::new(),
            detail: None,
        }
    }
}

/// Precision at K.
pub fn precision_at_k<L>(positive_label: L, k: usize) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(PrecisionAtK {
        positive: positive_label,
        k,
    })
}

struct PrecisionAtK<L> {
    positive: L,
    k: usize,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for PrecisionAtK<L> {
    fn name(&self) -> &str {
        "precision_at_k"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("precision_at_k");
        }

        let mut sorted: Vec<_> = scored.to_vec();
        sorted.sort_by(|a, b| {
            b.prediction
                .score
                .partial_cmp(&a.prediction.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let n = self.k.min(sorted.len());
        let relevant = sorted[..n]
            .iter()
            .filter(|s| s.sample.label == self.positive)
            .count();
        let val = safe_divide(relevant as f64, n as f64);

        let mut values = HashMap::new();
        values.insert("k".into(), n as f64);

        MetricResult {
            name: "precision_at_k".into(),
            value: val,
            values,
            detail: None,
        }
    }
}

/// Recall at K.
pub fn recall_at_k<L>(positive_label: L, k: usize) -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(RecallAtK {
        positive: positive_label,
        k,
    })
}

struct RecallAtK<L> {
    positive: L,
    k: usize,
}

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for RecallAtK<L> {
    fn name(&self) -> &str {
        "recall_at_k"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("recall_at_k");
        }

        let mut sorted: Vec<_> = scored.to_vec();
        sorted.sort_by(|a, b| {
            b.prediction
                .score
                .partial_cmp(&a.prediction.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_relevant = scored
            .iter()
            .filter(|s| s.sample.label == self.positive)
            .count();
        if total_relevant == 0 {
            return empty_result("recall_at_k");
        }

        let n = self.k.min(sorted.len());
        let relevant_in_k = sorted[..n]
            .iter()
            .filter(|s| s.sample.label == self.positive)
            .count();
        let val = safe_divide(relevant_in_k as f64, total_relevant as f64);

        let mut values = HashMap::new();
        values.insert("k".into(), n as f64);

        MetricResult {
            name: "recall_at_k".into(),
            value: val,
            values,
            detail: None,
        }
    }
}

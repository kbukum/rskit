//! Pluggable evaluation metrics for the bench framework.

mod adapter;
mod classification;
mod composite;
mod matching;
mod probability;
mod ranking;
mod regression;

pub use adapter::{RunMetric, as_run_metric, as_run_metrics};
pub use classification::{
    binary_classification, confusion_matrix, multi_class_classification, threshold_sweep,
};
pub use composite::weighted;
pub use matching::{exact_match, fuzzy_match};
pub use probability::{auc_roc, brier_score, calibration, log_loss};
pub use ranking::{mean_average_precision, ndcg, precision_at_k, recall_at_k};
pub use regression::{mae, mse, r_squared, rmse};

use crate::result::MetricResult;
use crate::types::ScoredSample;

pub trait Metric<L = String>: Send + Sync {
    fn name(&self) -> &str;
    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult;
}

pub struct Suite<L = String> {
    metrics: Vec<Box<dyn Metric<L>>>,
}

impl<L> Suite<L> {
    pub fn new(metrics: Vec<Box<dyn Metric<L>>>) -> Self {
        Self { metrics }
    }

    pub fn add(&mut self, metric: Box<dyn Metric<L>>) {
        self.metrics.push(metric);
    }

    pub fn compute(&self, scored: &[ScoredSample<L>]) -> Vec<MetricResult> {
        self.metrics.iter().map(|m| m.compute(scored)).collect()
    }
}

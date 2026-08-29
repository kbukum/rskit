use std::sync::Arc;

use crate::metric::AsyncMetric;
use crate::result::MetricResult;
use crate::types::ScoredSample;
use rskit_errors::AppResult;

/// Computes one benchmark metric from scored samples.
pub trait Metric<L = String>: Send + Sync {
    /// Returns the stable metric name used in benchmark results.
    fn name(&self) -> &str;
    /// Computes the metric result from scored samples.
    ///
    /// Returns an error when the metric cannot produce a faithful result — for example when an injected dependency such as a tokenizer fails — rather than fabricating a success-shaped or `NaN` value that would corrupt aggregate totals.
    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult>;
}

/// Ordered collection of metrics evaluated for a benchmark run.
///
/// A suite holds synchronous [`Metric`]s and asynchronous [`AsyncMetric`]s separately. [`compute`](Self::compute) evaluates only the synchronous, deterministic metrics; [`compute_all`](Self::compute_all) evaluates the synchronous metrics first and then awaits the asynchronous ones, merging results in a stable order (sync metrics in suite order, then async metrics in suite order) so a run's metric sequence is reproducible.
pub struct Suite<L = String> {
    metrics: Vec<Box<dyn Metric<L>>>,
    async_metrics: Vec<Arc<dyn AsyncMetric<L>>>,
}

impl<L> Suite<L> {
    /// Creates a metric suite from an ordered synchronous metric list.
    pub fn new(metrics: Vec<Box<dyn Metric<L>>>) -> Self {
        Self {
            metrics,
            async_metrics: Vec::new(),
        }
    }

    /// Appends a synchronous metric to the suite.
    pub fn add(&mut self, metric: Box<dyn Metric<L>>) {
        self.metrics.push(metric);
    }

    /// Appends an asynchronous metric to the suite.
    pub fn add_async(&mut self, metric: Arc<dyn AsyncMetric<L>>) {
        self.async_metrics.push(metric);
    }

    /// Computes every synchronous metric in suite order for the supplied scored samples.
    ///
    /// Fails fast if any metric returns an error, propagating it rather than recording a partial or fabricated result set. Asynchronous metrics are not evaluated here — use [`compute_all`](Self::compute_all) to include them.
    pub fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<Vec<MetricResult>> {
        self.metrics.iter().map(|m| m.compute(scored)).collect()
    }

    /// Computes synchronous metrics, then awaits asynchronous metrics, returning all results in a stable order.
    ///
    /// Synchronous metrics run first in suite order, then each asynchronous metric is awaited in suite order and its result appended. Fails fast on the first error from either phase, so a provider failure surfaces rather than yielding a partial result set.
    pub async fn compute_all(&self, scored: &[ScoredSample<L>]) -> AppResult<Vec<MetricResult>> {
        let mut results = self.compute(scored)?;
        for metric in &self.async_metrics {
            results.push(metric.compute(scored).await?);
        }
        Ok(results)
    }
}

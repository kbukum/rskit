use crate::result::MetricResult;
use crate::types::ScoredSample;
use rskit_errors::AppResult;

/// Computes one benchmark metric from scored samples.
pub trait Metric<L = String>: Send + Sync {
    /// Returns the stable metric name used in benchmark results.
    fn name(&self) -> &str;
    /// Computes the metric result from scored samples.
    ///
    /// Returns an error when the metric cannot produce a faithful result — for
    /// example when an injected dependency such as a tokenizer fails — rather
    /// than fabricating a success-shaped or `NaN` value that would corrupt
    /// aggregate totals.
    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult>;
}

/// Ordered collection of metrics evaluated for a benchmark run.
pub struct Suite<L = String> {
    metrics: Vec<Box<dyn Metric<L>>>,
}

impl<L> Suite<L> {
    /// Creates a metric suite from an ordered metric list.
    pub fn new(metrics: Vec<Box<dyn Metric<L>>>) -> Self {
        Self { metrics }
    }

    /// Appends a metric to the suite.
    pub fn add(&mut self, metric: Box<dyn Metric<L>>) {
        self.metrics.push(metric);
    }

    /// Computes every metric in suite order for the supplied scored samples.
    ///
    /// Fails fast if any metric returns an error, propagating it rather than
    /// recording a partial or fabricated result set.
    pub fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<Vec<MetricResult>> {
        self.metrics.iter().map(|m| m.compute(scored)).collect()
    }
}

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

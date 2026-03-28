use super::Metric;
use crate::result::MetricResult;
use crate::types::ScoredSample;

pub trait RunMetric<L = String>: Send + Sync {
    fn name(&self) -> &str;
    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult;
}

pub fn as_run_metric<L>(metric: Box<dyn Metric<L>>) -> Box<dyn RunMetric<L>>
where
    L: Send + Sync + 'static,
{
    Box::new(RunMetricAdapter(metric))
}

pub fn as_run_metrics<L>(metrics: Vec<Box<dyn Metric<L>>>) -> Vec<Box<dyn RunMetric<L>>>
where
    L: Send + Sync + 'static,
{
    metrics.into_iter().map(as_run_metric).collect()
}

struct RunMetricAdapter<L>(Box<dyn Metric<L>>);

impl<L: Send + Sync + 'static> RunMetric<L> for RunMetricAdapter<L> {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        self.0.compute(scored)
    }
}

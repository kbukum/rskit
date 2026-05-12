use super::{Metric, MetricResult, ScoredSample};
use std::collections::HashMap;

pub fn weighted<L>(metrics: Vec<(Box<dyn Metric<L>>, f64)>) -> Box<dyn Metric<L>>
where
    L: Send + Sync + 'static,
{
    Box::new(Weighted { metrics })
}

struct Weighted<L> {
    metrics: Vec<(Box<dyn Metric<L>>, f64)>,
}

impl<L: Send + Sync + 'static> Metric<L> for Weighted<L> {
    fn name(&self) -> &str {
        "weighted"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;
        let mut values = HashMap::new();

        for (metric, weight) in &self.metrics {
            let result = metric.compute(scored);
            values.insert(result.name.clone(), result.value);
            weighted_sum += result.value * weight;
            total_weight += weight;
        }

        let value = if total_weight == 0.0 {
            0.0
        } else {
            weighted_sum / total_weight
        };

        MetricResult {
            name: "weighted".into(),
            value,
            values,
            detail: None,
        }
    }
}

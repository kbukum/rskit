use super::Metric;
use crate::result::MetricResult;
use crate::types::ScoredSample;
use rskit_errors::AppResult;

/// Object-safe metric interface used by run-level benchmark orchestration.
pub trait RunMetric<L = String>: Send + Sync {
    /// Returns the stable metric name used in benchmark results.
    fn name(&self) -> &str;
    /// Computes the metric from scored samples.
    ///
    /// Propagates an error when the underlying metric cannot produce a faithful
    /// result rather than fabricating a value.
    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult>;
}

/// Adapts a metric suite metric into the run-level metric trait.
pub fn as_run_metric<L>(metric: Box<dyn Metric<L>>) -> Box<dyn RunMetric<L>>
where
    L: Send + Sync + 'static,
{
    Box::new(RunMetricAdapter(metric))
}

/// Adapts every metric in a vector into the run-level metric trait.
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

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        self.0.compute(scored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BenchSample, Prediction};
    use std::collections::HashMap;

    struct Stub;

    impl Metric<String> for Stub {
        fn name(&self) -> &str {
            "stub"
        }

        fn compute(&self, scored: &[ScoredSample<String>]) -> AppResult<MetricResult> {
            Ok(MetricResult {
                name: "stub".into(),
                value: scored.len() as f64,
                values: HashMap::new(),
                detail: None,
            })
        }
    }

    fn sample() -> ScoredSample<String> {
        ScoredSample {
            sample: BenchSample {
                id: "s1".into(),
                input: vec![],
                label: "a".into(),
                source: String::new(),
                metadata: HashMap::new(),
            },
            prediction: Prediction {
                sample_id: "s1".into(),
                label: "a".into(),
                score: 1.0,
                scores: HashMap::new(),
                metadata: HashMap::new(),
            },
        }
    }

    #[test]
    fn as_run_metric_delegates_name_and_compute() {
        let run_metric = as_run_metric(Box::new(Stub));
        let result = run_metric.compute(&[sample(), sample()]).unwrap();

        assert_eq!(run_metric.name(), "stub");
        assert_eq!(result.name, "stub");
        assert_eq!(result.value, 2.0);
    }

    #[test]
    fn as_run_metrics_adapts_every_metric() {
        let metrics: Vec<Box<dyn Metric<String>>> = vec![Box::new(Stub), Box::new(Stub)];
        let run_metrics = as_run_metrics(metrics);

        assert_eq!(run_metrics.len(), 2);
        for run_metric in &run_metrics {
            assert_eq!(run_metric.name(), "stub");
            assert_eq!(run_metric.compute(&[sample()]).unwrap().value, 1.0);
        }
    }
}

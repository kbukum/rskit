use super::Metric;
use crate::{MetricDirection, MetricResult, ScoredSample};
use rskit_errors::AppResult;
use std::collections::HashMap;

/// Creates a composite metric that averages constituent metric values by their supplied weights.
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

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;
        let mut values = HashMap::new();

        for (metric, weight) in &self.metrics {
            let result = metric.compute(scored)?;
            values.insert(result.name.clone(), result.value);
            weighted_sum += result.value * weight;
            total_weight += weight;
        }

        let value = if total_weight == 0.0 {
            0.0
        } else {
            weighted_sum / total_weight
        };

        Ok(MetricResult {
            name: "weighted".into(),
            value,
            direction: MetricDirection::HigherIsBetter,
            values,
            detail: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BenchSample, Prediction, ScoredSample};

    struct Constant {
        name: &'static str,
        value: f64,
    }

    impl Metric<String> for Constant {
        fn name(&self) -> &str {
            self.name
        }

        fn compute(&self, _scored: &[ScoredSample<String>]) -> AppResult<MetricResult> {
            Ok(MetricResult {
                name: self.name.into(),
                value: self.value,
                ..Default::default()
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
    fn weighted_average_combines_constituent_metrics() {
        let metric = weighted::<String>(vec![
            (
                Box::new(Constant {
                    name: "p",
                    value: 1.0,
                }),
                3.0,
            ),
            (
                Box::new(Constant {
                    name: "r",
                    value: 0.0,
                }),
                1.0,
            ),
        ]);
        let result = metric.compute(&[sample()]).unwrap();

        assert_eq!(metric.name(), "weighted");
        assert_eq!(result.name, "weighted");
        // (1.0*3 + 0.0*1) / (3 + 1) = 0.75
        assert!((result.value - 0.75).abs() < 1e-9);
        assert_eq!(result.values.get("p"), Some(&1.0));
        assert_eq!(result.values.get("r"), Some(&0.0));
    }

    #[test]
    fn zero_total_weight_yields_zero_without_dividing() {
        let metric = weighted::<String>(vec![(
            Box::new(Constant {
                name: "p",
                value: 0.9,
            }),
            0.0,
        )]);
        let result = metric.compute(&[sample()]).unwrap();

        assert_eq!(result.value, 0.0);
        assert_eq!(result.values.get("p"), Some(&0.9));
    }

    #[test]
    fn no_constituent_metrics_yields_zero() {
        let metric = weighted::<String>(vec![]);
        let result = metric.compute(&[sample()]).unwrap();

        assert_eq!(result.value, 0.0);
        assert!(result.values.is_empty());
    }
}

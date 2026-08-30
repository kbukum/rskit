use super::Metric;
use crate::{MetricDirection, MetricResult, ScoredSample};
use rskit_errors::AppResult;
use std::collections::HashMap;

fn safe_divide(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

fn empty_result(name: &str, direction: MetricDirection) -> MetricResult {
    MetricResult {
        directions: Default::default(),
        name: name.into(),
        value: 0.0,
        direction,
        values: HashMap::new(),
        detail: None,
    }
}

/// Mean Absolute Error.
pub fn mae() -> Box<dyn Metric<f64>> {
    Box::new(Mae)
}

struct Mae;

impl Metric<f64> for Mae {
    fn name(&self) -> &str {
        "mae"
    }

    fn compute(&self, scored: &[ScoredSample<f64>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(empty_result("mae", MetricDirection::LowerIsBetter));
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).abs())
            .sum();
        Ok(MetricResult {
            directions: Default::default(),
            name: "mae".into(),
            value: sum / scored.len() as f64,
            direction: MetricDirection::LowerIsBetter,
            values: HashMap::new(),
            detail: None,
        })
    }
}

/// Mean Squared Error.
pub fn mse() -> Box<dyn Metric<f64>> {
    Box::new(Mse)
}

struct Mse;

impl Metric<f64> for Mse {
    fn name(&self) -> &str {
        "mse"
    }

    fn compute(&self, scored: &[ScoredSample<f64>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(empty_result("mse", MetricDirection::LowerIsBetter));
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).powi(2))
            .sum();
        Ok(MetricResult {
            directions: Default::default(),
            name: "mse".into(),
            value: sum / scored.len() as f64,
            direction: MetricDirection::LowerIsBetter,
            values: HashMap::new(),
            detail: None,
        })
    }
}

/// Root Mean Squared Error.
pub fn rmse() -> Box<dyn Metric<f64>> {
    Box::new(Rmse)
}

struct Rmse;

impl Metric<f64> for Rmse {
    fn name(&self) -> &str {
        "rmse"
    }

    fn compute(&self, scored: &[ScoredSample<f64>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(empty_result("rmse", MetricDirection::LowerIsBetter));
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).powi(2))
            .sum();
        let val = (sum / scored.len() as f64).sqrt();
        Ok(MetricResult {
            directions: Default::default(),
            name: "rmse".into(),
            value: val,
            direction: MetricDirection::LowerIsBetter,
            values: HashMap::new(),
            detail: None,
        })
    }
}

/// R-squared (coefficient of determination).
pub fn r_squared() -> Box<dyn Metric<f64>> {
    Box::new(RSquared)
}

struct RSquared;

impl Metric<f64> for RSquared {
    fn name(&self) -> &str {
        "r_squared"
    }

    fn compute(&self, scored: &[ScoredSample<f64>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(empty_result("r_squared", MetricDirection::HigherIsBetter));
        }
        let mean: f64 = scored.iter().map(|s| s.sample.label).sum::<f64>() / scored.len() as f64;
        let ss_res: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).powi(2))
            .sum();
        let ss_tot: f64 = scored.iter().map(|s| (s.sample.label - mean).powi(2)).sum();
        let val = 1.0 - safe_divide(ss_res, ss_tot);
        let mut values = HashMap::new();
        values.insert("ss_res".into(), ss_res);
        values.insert("ss_tot".into(), ss_tot);
        // R² itself is higher-is-better; a lower residual sum of squares is
        // better, while the total sum of squares is a descriptive property of the
        // data.
        let mut directions = HashMap::new();
        directions.insert("ss_res".into(), MetricDirection::LowerIsBetter);
        directions.insert("ss_tot".into(), MetricDirection::Neutral);
        Ok(MetricResult {
            name: "r_squared".into(),
            value: val,
            direction: MetricDirection::HigherIsBetter,
            directions,
            values,
            detail: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BenchSample, Prediction, ScoredSample};

    fn scored(label: f64, score: f64) -> ScoredSample<f64> {
        ScoredSample {
            sample: BenchSample {
                id: "s".into(),
                input: vec![],
                label,
                source: String::new(),
                metadata: HashMap::new(),
            },
            prediction: Prediction {
                sample_id: "s".into(),
                label,
                score,
                scores: HashMap::new(),
                metadata: HashMap::new(),
            },
        }
    }

    #[test]
    fn r_squared_subvalue_directions() {
        let r = r_squared()
            .compute(&[scored(1.0, 1.2), scored(2.0, 1.8)])
            .unwrap();
        assert_eq!(r.direction, MetricDirection::HigherIsBetter);
        assert_eq!(
            r.directions.get("ss_res"),
            Some(&MetricDirection::LowerIsBetter)
        );
        assert_eq!(r.directions.get("ss_tot"), Some(&MetricDirection::Neutral));
    }
}

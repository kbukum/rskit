use super::{Metric, MetricResult, ScoredSample};
use std::collections::HashMap;

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

/// Mean Absolute Error.
pub fn mae() -> Box<dyn Metric<f64>> {
    Box::new(Mae)
}

struct Mae;

impl Metric<f64> for Mae {
    fn name(&self) -> &str {
        "mae"
    }

    fn compute(&self, scored: &[ScoredSample<f64>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("mae");
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).abs())
            .sum();
        MetricResult {
            name: "mae".into(),
            value: sum / scored.len() as f64,
            values: HashMap::new(),
            detail: None,
        }
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

    fn compute(&self, scored: &[ScoredSample<f64>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("mse");
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).powi(2))
            .sum();
        MetricResult {
            name: "mse".into(),
            value: sum / scored.len() as f64,
            values: HashMap::new(),
            detail: None,
        }
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

    fn compute(&self, scored: &[ScoredSample<f64>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("rmse");
        }
        let sum: f64 = scored
            .iter()
            .map(|s| (s.sample.label - s.prediction.score).powi(2))
            .sum();
        let val = (sum / scored.len() as f64).sqrt();
        MetricResult {
            name: "rmse".into(),
            value: val,
            values: HashMap::new(),
            detail: None,
        }
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

    fn compute(&self, scored: &[ScoredSample<f64>]) -> MetricResult {
        if scored.is_empty() {
            return empty_result("r_squared");
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
        MetricResult {
            name: "r_squared".into(),
            value: val,
            values,
            detail: None,
        }
    }
}

use super::Metric;
use super::identity::format_threshold;
use crate::{MetricDirection, MetricResult, ScoredSample};
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashMap;
use std::fmt::Display;
use std::marker::PhantomData;

fn safe_divide(a: f64, b: f64) -> f64 {
    if b == 0.0 { 0.0 } else { a / b }
}

/// Exact match metric: fraction of predictions that exactly match the label.
pub fn exact_match<L>() -> Box<dyn Metric<L>>
where
    L: PartialEq + Clone + Send + Sync + 'static,
{
    Box::new(ExactMatch::<L>(PhantomData))
}

struct ExactMatch<L>(PhantomData<L>);

impl<L: PartialEq + Clone + Send + Sync + 'static> Metric<L> for ExactMatch<L> {
    fn name(&self) -> &str {
        "exact_match"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(MetricResult {
                directions: Default::default(),
                name: "exact_match".into(),
                value: 0.0,
                direction: MetricDirection::HigherIsBetter,
                values: HashMap::new(),
                detail: None,
            });
        }
        let correct = scored
            .iter()
            .filter(|s| s.sample.label == s.prediction.label)
            .count();
        let val = safe_divide(correct as f64, scored.len() as f64);
        let mut values = HashMap::new();
        values.insert("correct".into(), correct as f64);
        values.insert("total".into(), scored.len() as f64);
        Ok(MetricResult {
            directions: Default::default(),
            name: "exact_match".into(),
            value: val,
            direction: MetricDirection::HigherIsBetter,
            values,
            detail: None,
        })
    }
}

/// Fuzzy match metric using Levenshtein distance (string labels only).
///
/// The configured threshold is folded into the metric name (for example `fuzzy_match[t0.7]`) and recorded in [`MetricResult::detail`] provenance rather than [`MetricResult::values`]: it is a configuration input, not a quality signal. Because `match_rate` (the primary value) is a fraction at this cutoff, embedding the threshold in the identity keeps runs scored at different thresholds distinct under [`RunComparator`](crate::compare::RunComparator) instead of joining them and scoring an incomparable delta as an improvement or regression. The threshold must be finite and in `[0, 1]`; an invalid value is rejected as a typed [`ErrorCode::InvalidInput`] error when the metric is computed, rather than yielding a `tNaN` identity and a `null` provenance value.
pub fn fuzzy_match<L>(threshold: f64) -> Box<dyn Metric<L>>
where
    L: Display + Clone + Send + Sync + 'static,
{
    Box::new(FuzzyMatch::<L> {
        threshold,
        name: build_name(threshold),
        _phantom: PhantomData,
    })
}

/// Builds the comparison-safe metric name from the configured threshold.
///
/// The threshold is part of the identity because `match_rate` (the primary value) is a fraction at a fixed cutoff: two runs scored at different thresholds must never join under a shared name, or [`RunComparator`](crate::compare::RunComparator) would score an incomparable pass-rate delta as a regression or improvement.
fn build_name(threshold: f64) -> String {
    format!("fuzzy_match[t{}]", format_threshold(threshold))
}

struct FuzzyMatch<L> {
    threshold: f64,
    name: String,
    _phantom: PhantomData<L>,
}

/// Compute Levenshtein distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];

    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Compute similarity from Levenshtein distance (0.0 to 1.0).
fn similarity(a: &str, b: &str) -> f64 {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    1.0 - (levenshtein(a, b) as f64 / max_len as f64)
}

impl<L: Display + Clone + Send + Sync + 'static> Metric<L> for FuzzyMatch<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        if !self.threshold.is_finite() || !(0.0..=1.0).contains(&self.threshold) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "fuzzy_match: threshold {} is out of the required range [0, 1]",
                    self.threshold
                ),
            ));
        }
        if scored.is_empty() {
            return Ok(MetricResult {
                directions: Default::default(),
                name: self.name.clone(),
                value: 0.0,
                direction: MetricDirection::HigherIsBetter,
                values: HashMap::new(),
                detail: Some(serde_json::json!({ "threshold": self.threshold })),
            });
        }

        let mut total_sim = 0.0;
        let mut matches = 0usize;
        for s in scored {
            let actual = format!("{}", s.sample.label);
            let predicted = format!("{}", s.prediction.label);
            let sim = similarity(&actual, &predicted);
            total_sim += sim;
            if sim >= self.threshold {
                matches += 1;
            }
        }

        let avg_sim = safe_divide(total_sim, scored.len() as f64);
        let match_rate = safe_divide(matches as f64, scored.len() as f64);

        let mut values = HashMap::new();
        values.insert("average_similarity".into(), avg_sim);
        values.insert("match_rate".into(), match_rate);

        // The threshold is a configuration input, not a quality signal, so it lives in provenance detail rather than `values`, where `RunComparator` would score a threshold change as an improvement or regression.
        Ok(MetricResult {
            directions: Default::default(),
            name: self.name.clone(),
            value: match_rate,
            direction: MetricDirection::HigherIsBetter,
            values,
            detail: Some(serde_json::json!({ "threshold": self.threshold })),
        })
    }
}

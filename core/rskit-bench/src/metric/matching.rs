use super::Metric;
use crate::{MetricResult, ScoredSample};
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

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return MetricResult {
                name: "exact_match".into(),
                value: 0.0,
                values: HashMap::new(),
                detail: None,
            };
        }
        let correct = scored
            .iter()
            .filter(|s| s.sample.label == s.prediction.label)
            .count();
        let val = safe_divide(correct as f64, scored.len() as f64);
        let mut values = HashMap::new();
        values.insert("correct".into(), correct as f64);
        values.insert("total".into(), scored.len() as f64);
        MetricResult {
            name: "exact_match".into(),
            value: val,
            values,
            detail: None,
        }
    }
}

/// Fuzzy match metric using Levenshtein distance (string labels only).
pub fn fuzzy_match<L>(threshold: f64) -> Box<dyn Metric<L>>
where
    L: Display + Clone + Send + Sync + 'static,
{
    Box::new(FuzzyMatch::<L> {
        threshold,
        _phantom: PhantomData,
    })
}

struct FuzzyMatch<L> {
    threshold: f64,
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
        "fuzzy_match"
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return MetricResult {
                name: "fuzzy_match".into(),
                value: 0.0,
                values: HashMap::new(),
                detail: None,
            };
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
        values.insert("threshold".into(), self.threshold);

        MetricResult {
            name: "fuzzy_match".into(),
            value: match_rate,
            values,
            detail: None,
        }
    }
}

//! Token-usage metric backed by an injected [`TokenCounter`].
//!
//! Reports how many tokens the predicted and reference labels decompose into,
//! using a caller-supplied [`TokenCounter`] so the tokenization strategy
//! (heuristic core default or a real `contrib/llm` adapter) is chosen by
//! injection rather than wired in here.

use super::Metric;
use crate::{MetricResult, ScoredSample};
use rskit_llm::TokenCounter;
use std::collections::HashMap;
use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;

/// Reports predicted and reference token usage using an injected counter.
///
/// The metric renders each prediction and ground-truth label to text via
/// [`Display`] and counts tokens with `counter`. It reports average predicted
/// tokens as the primary [`MetricResult::value`] and records totals and
/// averages for both predicted and reference tokens in
/// [`MetricResult::values`]. Empty input yields a zeroed result rather than a
/// panic. A tokenization failure surfaces as a `NaN`-valued result carrying the
/// error in [`MetricResult::detail`] rather than a fabricated count that would
/// corrupt aggregate totals.
pub fn token_stats<L>(counter: Arc<dyn TokenCounter>) -> Box<dyn Metric<L>>
where
    L: Display + Clone + Send + Sync + 'static,
{
    Box::new(TokenStats {
        counter,
        _phantom: PhantomData,
    })
}

struct TokenStats<L> {
    counter: Arc<dyn TokenCounter>,
    _phantom: PhantomData<L>,
}

const NAME: &str = "token_stats";

impl<L: Display + Clone + Send + Sync + 'static> Metric<L> for TokenStats<L> {
    fn name(&self) -> &str {
        NAME
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> MetricResult {
        if scored.is_empty() {
            return zeroed_result();
        }

        let mut predicted_total = 0usize;
        let mut reference_total = 0usize;
        for sample in scored {
            let predicted = match self.counter.count(&sample.prediction.label.to_string()) {
                Ok(tokens) => tokens,
                Err(err) => return failed_result(&err),
            };
            let reference = match self.counter.count(&sample.sample.label.to_string()) {
                Ok(tokens) => tokens,
                Err(err) => return failed_result(&err),
            };
            predicted_total += predicted;
            reference_total += reference;
        }

        let count = scored.len() as f64;
        let predicted_avg = predicted_total as f64 / count;
        let reference_avg = reference_total as f64 / count;

        let mut values = HashMap::new();
        values.insert("predicted_tokens_total".into(), predicted_total as f64);
        values.insert("predicted_tokens_avg".into(), predicted_avg);
        values.insert("reference_tokens_total".into(), reference_total as f64);
        values.insert("reference_tokens_avg".into(), reference_avg);

        MetricResult {
            name: NAME.into(),
            value: predicted_avg,
            values,
            detail: None,
        }
    }
}

fn zeroed_result() -> MetricResult {
    let mut values = HashMap::new();
    values.insert("predicted_tokens_total".into(), 0.0);
    values.insert("predicted_tokens_avg".into(), 0.0);
    values.insert("reference_tokens_total".into(), 0.0);
    values.insert("reference_tokens_avg".into(), 0.0);
    MetricResult {
        name: NAME.into(),
        value: 0.0,
        values,
        detail: None,
    }
}

/// Surfaces a tokenization failure without fabricating a token count.
///
/// The [`Metric`] contract is infallible, so a counting error is reported as a
/// `NaN`-valued result carrying the error in `detail`. This keeps a failed
/// tokenization visible instead of collapsing it into a success-shaped zero
/// that would silently corrupt aggregate totals.
fn failed_result(err: &rskit_errors::AppError) -> MetricResult {
    MetricResult {
        name: NAME.into(),
        value: f64::NAN,
        values: HashMap::new(),
        detail: Some(serde_json::json!({ "error": err.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BenchSample, Prediction};
    use rskit_llm::HeuristicTokenCounter;

    fn scored(prediction: &str, reference: &str) -> ScoredSample<String> {
        ScoredSample {
            sample: BenchSample {
                id: "s".into(),
                input: Vec::new(),
                label: reference.to_string(),
                source: String::new(),
                metadata: HashMap::new(),
            },
            prediction: Prediction {
                sample_id: "s".into(),
                label: prediction.to_string(),
                score: 1.0,
                scores: HashMap::new(),
                metadata: HashMap::new(),
            },
        }
    }

    #[test]
    fn empty_input_is_zeroed_not_panicking() {
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        let result = metric.compute(&[]);
        assert_eq!(result.value, 0.0);
        assert_eq!(result.values["predicted_tokens_total"], 0.0);
        assert_eq!(result.values["reference_tokens_total"], 0.0);
    }

    #[test]
    fn counts_predicted_and_reference_tokens() {
        // "abcd" -> 1 token, "abcdefgh" -> 2 tokens under the heuristic.
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        let samples = vec![scored("abcd", "abcdefgh")];
        let result = metric.compute(&samples);
        assert_eq!(result.values["predicted_tokens_total"], 1.0);
        assert_eq!(result.values["reference_tokens_total"], 2.0);
        assert_eq!(result.value, 1.0);
    }

    #[test]
    fn averages_across_samples() {
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        let samples = vec![scored("abcd", "abcd"), scored("abcdefgh", "abcd")];
        let result = metric.compute(&samples);
        // predicted totals: 1 + 2 = 3 over 2 samples -> avg 1.5
        assert_eq!(result.values["predicted_tokens_total"], 3.0);
        assert_eq!(result.values["predicted_tokens_avg"], 1.5);
        assert_eq!(result.value, 1.5);
    }

    #[test]
    fn injected_counter_is_used() {
        // A fixed counter proves the metric defers to the injected strategy.
        struct FixedCounter(usize);
        impl TokenCounter for FixedCounter {
            fn count(&self, _text: &str) -> rskit_errors::AppResult<usize> {
                Ok(self.0)
            }
        }
        let metric = token_stats::<String>(Arc::new(FixedCounter(7)));
        let samples = vec![scored("x", "y")];
        let result = metric.compute(&samples);
        assert_eq!(result.values["predicted_tokens_total"], 7.0);
        assert_eq!(result.values["reference_tokens_total"], 7.0);
    }

    #[test]
    fn counting_failure_is_surfaced_not_zeroed() {
        // A failing counter must not collapse into a success-shaped zero: the
        // result is NaN and carries the error in `detail`.
        struct FailingCounter;
        impl TokenCounter for FailingCounter {
            fn count(&self, _text: &str) -> rskit_errors::AppResult<usize> {
                Err(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    "tokenizer exploded",
                ))
            }
        }
        let metric = token_stats::<String>(Arc::new(FailingCounter));
        let samples = vec![scored("x", "y")];
        let result = metric.compute(&samples);
        assert!(result.value.is_nan());
        assert!(result.values.is_empty());
        let detail = result.detail.expect("failure detail present");
        assert!(
            detail["error"]
                .as_str()
                .unwrap()
                .contains("tokenizer exploded")
        );
    }

    #[test]
    fn metric_name_is_stable() {
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        assert_eq!(metric.name(), "token_stats");
    }
}

//! Token-usage metric backed by an injected [`TokenCounter`].
//!
//! Reports how many tokens the predicted and reference labels decompose into,
//! using a caller-supplied [`TokenCounter`] so the tokenization strategy
//! (heuristic core default or a real `contrib/llm` adapter) is chosen by
//! injection rather than wired in here.

use super::Metric;
use crate::{MetricDirection, MetricResult, ScoredSample};
use rskit_errors::AppResult;
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
/// panic. A tokenization failure is propagated as an error rather than
/// collapsed into a fabricated count that would corrupt aggregate totals.
///
/// The metric name embeds the counter's [`TokenCounter::id`] (for example
/// `token_stats[tiktoken:cl100k_base]`) and the identity is recorded in
/// [`MetricResult::detail`], so runs tokenized by incompatible strategies stay
/// distinct in provenance and are not compared as if equivalent. Token usage is
/// descriptive, so the result is [`MetricDirection::Neutral`] — comparison never
/// flags a change as an improvement or a regression.
pub fn token_stats<L>(counter: Arc<dyn TokenCounter>) -> Box<dyn Metric<L>>
where
    L: Display + Clone + Send + Sync + 'static,
{
    let counter_id = counter.id();
    let name = format!("{BASE_NAME}[{counter_id}]");
    Box::new(TokenStats {
        counter,
        counter_id,
        name,
        _phantom: PhantomData,
    })
}

struct TokenStats<L> {
    counter: Arc<dyn TokenCounter>,
    counter_id: String,
    name: String,
    _phantom: PhantomData<L>,
}

const BASE_NAME: &str = "token_stats";

impl<L: Display + Clone + Send + Sync + 'static> Metric<L> for TokenStats<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(self.zeroed_result());
        }

        let mut predicted_total = 0usize;
        let mut reference_total = 0usize;
        for sample in scored {
            predicted_total += self.counter.count(&sample.prediction.label.to_string())?;
            reference_total += self.counter.count(&sample.sample.label.to_string())?;
        }

        let count = scored.len() as f64;
        let predicted_avg = predicted_total as f64 / count;
        let reference_avg = reference_total as f64 / count;

        let mut values = HashMap::new();
        values.insert("predicted_tokens_total".into(), predicted_total as f64);
        values.insert("predicted_tokens_avg".into(), predicted_avg);
        values.insert("reference_tokens_total".into(), reference_total as f64);
        values.insert("reference_tokens_avg".into(), reference_avg);

        Ok(MetricResult {
            directions: Default::default(),
            name: self.name.clone(),
            value: predicted_avg,
            direction: MetricDirection::Neutral,
            values,
            detail: Some(self.provenance()),
        })
    }
}

impl<L> TokenStats<L> {
    fn zeroed_result(&self) -> MetricResult {
        let mut values = HashMap::new();
        values.insert("predicted_tokens_total".into(), 0.0);
        values.insert("predicted_tokens_avg".into(), 0.0);
        values.insert("reference_tokens_total".into(), 0.0);
        values.insert("reference_tokens_avg".into(), 0.0);
        MetricResult {
            directions: Default::default(),
            name: self.name.clone(),
            value: 0.0,
            direction: MetricDirection::Neutral,
            values,
            detail: Some(self.provenance()),
        }
    }

    /// Records the injected counter's identity so a persisted result carries the
    /// tokenization provenance, not just the counts.
    fn provenance(&self) -> serde_json::Value {
        serde_json::json!({ "counter": self.counter_id })
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
        let result = metric.compute(&[]).unwrap();
        assert_eq!(result.value, 0.0);
        assert_eq!(result.values["predicted_tokens_total"], 0.0);
        assert_eq!(result.values["reference_tokens_total"], 0.0);
    }

    #[test]
    fn counts_predicted_and_reference_tokens() {
        // "abcd" -> 1 token, "abcdefgh" -> 2 tokens under the heuristic.
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        let samples = vec![scored("abcd", "abcdefgh")];
        let result = metric.compute(&samples).unwrap();
        assert_eq!(result.values["predicted_tokens_total"], 1.0);
        assert_eq!(result.values["reference_tokens_total"], 2.0);
        assert_eq!(result.value, 1.0);
    }

    #[test]
    fn averages_across_samples() {
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        let samples = vec![scored("abcd", "abcd"), scored("abcdefgh", "abcd")];
        let result = metric.compute(&samples).unwrap();
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
            fn id(&self) -> String {
                "fixed".to_string()
            }
        }
        let metric = token_stats::<String>(Arc::new(FixedCounter(7)));
        let samples = vec![scored("x", "y")];
        let result = metric.compute(&samples).unwrap();
        assert_eq!(result.values["predicted_tokens_total"], 7.0);
        assert_eq!(result.values["reference_tokens_total"], 7.0);
    }

    #[test]
    fn counting_failure_is_surfaced_not_zeroed() {
        // A failing counter must not collapse into a success-shaped zero: the
        // error propagates out of `compute` instead.
        struct FailingCounter;
        impl TokenCounter for FailingCounter {
            fn count(&self, _text: &str) -> rskit_errors::AppResult<usize> {
                Err(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    "tokenizer exploded",
                ))
            }
            fn id(&self) -> String {
                "failing".to_string()
            }
        }
        let metric = token_stats::<String>(Arc::new(FailingCounter));
        let samples = vec![scored("x", "y")];
        let err = metric
            .compute(&samples)
            .expect_err("counting failure must propagate");
        assert_eq!(err.code(), rskit_errors::ErrorCode::Internal);
        assert!(err.to_string().contains("tokenizer exploded"));
    }

    #[test]
    fn metric_name_embeds_counter_identity() {
        // The name carries the counter id so runs tokenized differently do not
        // collide under a single "token_stats" name.
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        assert_eq!(metric.name(), "token_stats[heuristic]");
    }

    #[test]
    fn result_is_neutral_and_records_provenance() {
        let metric = token_stats::<String>(Arc::new(HeuristicTokenCounter));
        // Both the populated and the empty paths carry direction + provenance.
        for result in [
            metric.compute(&[scored("abcd", "abcd")]).unwrap(),
            metric.compute(&[]).unwrap(),
        ] {
            assert_eq!(result.direction, MetricDirection::Neutral);
            assert_eq!(result.name, "token_stats[heuristic]");
            let detail = result.detail.expect("provenance detail present");
            assert_eq!(detail["counter"], "heuristic");
        }
    }

    #[test]
    fn distinct_counters_produce_distinct_metric_names() {
        // Two counters with different identities must not share a metric name,
        // so run comparison never treats them as the same metric.
        struct Fixed(&'static str);
        impl TokenCounter for Fixed {
            fn count(&self, _text: &str) -> rskit_errors::AppResult<usize> {
                Ok(1)
            }
            fn id(&self) -> String {
                self.0.to_string()
            }
        }
        let a = token_stats::<String>(Arc::new(Fixed("a")));
        let b = token_stats::<String>(Arc::new(Fixed("b")));
        assert_ne!(a.name(), b.name());
    }
}

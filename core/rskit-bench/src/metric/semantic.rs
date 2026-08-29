//! Semantic-similarity metric backed by an injected embedding provider.
//!
//! [`semantic_similarity`] scores each prediction against its reference by embedding both texts through an injected [`rskit_embedding::Provider`] and taking their cosine similarity, rather than comparing surface strings. It is an [`AsyncMetric`] because embedding is I/O-backed; every provider call runs through an injected [`rskit_resilience::Policy`] (a per-call timeout by default) so a slow or hung provider cannot stall a run, and any provider failure surfaces as a typed error instead of a fabricated score. The metric name embeds the embedding model's identity and the result records it in provenance, so runs scored with incompatible models stay distinct under comparison instead of being joined by a shared name.

use std::collections::HashMap;
use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_ai::Provider as ModelProvider;
use rskit_ai::vector::cosine_similarity;
use rskit_embedding::{EmbedInput, EmbedRequest, EmbeddingOptions, Model, Provider};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_resilience::Policy;

use super::AsyncMetric;
use super::identity::escape_component;
use crate::{MetricDirection, MetricResult, ScoredSample};

/// Base metric name; the embedding model's identity is appended to form the full, comparison-safe name.
const NAME: &str = "semantic_similarity";
/// Default cosine-similarity threshold at or above which a pair counts as a match.
const DEFAULT_THRESHOLD: f32 = 0.8;
/// Default per-call embedding timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default number of samples embedded per provider call.
///
/// Each sample contributes two inputs (reference + prediction), so the default bounds a request to 128 inputs — comfortably under common provider batch limits — rather than embedding an entire dataset in one unbounded call.
const DEFAULT_BATCH_SIZE: usize = 64;

/// Renders a label to the text embedded for similarity scoring.
type TextExtractor<L> = Arc<dyn Fn(&L) -> String + Send + Sync>;

/// Creates a semantic-similarity metric over an injected embedding provider.
///
/// The metric embeds each sample's reference and prediction labels (rendered to text via the label's [`Display`] by default) and scores them by cosine similarity. It reports the average similarity as the primary [`MetricResult::value`] and records the average, the match rate at the configured threshold, and the threshold itself in [`MetricResult::values`].
///
/// The metric name embeds the embedding model's identity (for example `semantic_similarity[open_ai/text-embedding-3-small]`) and that identity is recorded in [`MetricResult::detail`], so runs scored with incompatible embedding models stay distinct in provenance and are never joined by [`RunComparator`](crate::compare::RunComparator) as if comparable.
///
/// Samples are embedded in bounded batches (see [`with_batch_size`](SemanticSimilarity::with_batch_size)); each provider call runs through an injected [`rskit_resilience::Policy`] (see [`with_policy`](SemanticSimilarity::with_policy)) whose default is a per-call timeout, so a large run neither exceeds provider batch limits nor rides on a single dataset-wide deadline.
///
/// Tune it with [`with_threshold`](SemanticSimilarity::with_threshold), [`with_timeout`](SemanticSimilarity::with_timeout) or a full [`with_policy`](SemanticSimilarity::with_policy), [`with_batch_size`](SemanticSimilarity::with_batch_size), and [`with_extractor`](SemanticSimilarity::with_extractor), then add it to a [`Suite`](crate::metric::Suite) with [`add_async`](crate::metric::Suite::add_async).
pub fn semantic_similarity<L>(provider: Arc<dyn Provider>, model: Model) -> SemanticSimilarity<L>
where
    L: Display + Send + Sync + 'static,
{
    let model_id = model_identity(&model);
    let name = format!("{NAME}[{model_id}]");
    SemanticSimilarity {
        provider,
        model,
        model_id,
        name,
        extract: Arc::new(|label: &L| label.to_string()),
        threshold: DEFAULT_THRESHOLD,
        policy: Policy::new().with_timeout(DEFAULT_TIMEOUT),
        batch_size: DEFAULT_BATCH_SIZE,
        _phantom: PhantomData,
    }
}

/// Renders an embedding model to a stable identity string used in the metric name and provenance, so incompatible models never collide under a shared name.
fn model_identity(model: &Model) -> String {
    let provider = escape_component(&provider_tag(&model.provider));
    let name = escape_component(&model.name);
    match &model.version {
        Some(version) => format!("{provider}/{name}@{}", escape_component(version)),
        None => format!("{provider}/{name}"),
    }
}

/// Renders a model provider to a stable, human-readable tag.
fn provider_tag(provider: &ModelProvider) -> String {
    match provider {
        ModelProvider::Custom(name) => name.clone(),
        // Unit variants serialize to their canonical snake_case tag.
        other => match serde_json::to_value(other) {
            Ok(serde_json::Value::String(tag)) => tag,
            _ => format!("{other:?}"),
        },
    }
}

/// Embedding-cosine similarity metric produced by [`semantic_similarity`].
pub struct SemanticSimilarity<L> {
    provider: Arc<dyn Provider>,
    model: Model,
    model_id: String,
    name: String,
    extract: TextExtractor<L>,
    threshold: f32,
    policy: Policy,
    batch_size: usize,
    _phantom: PhantomData<fn(&L)>,
}

impl<L> SemanticSimilarity<L> {
    /// Sets the cosine-similarity threshold at or above which a pair counts as a match.
    #[must_use]
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Sets the per-call embedding timeout on the resilience policy.
    ///
    /// Convenience over [`with_policy`](Self::with_policy) that adjusts only the timeout of the current [`Policy`], leaving any other configured primitives (retries, circuit-breaker, …) intact.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.policy = std::mem::take(&mut self.policy).with_timeout(timeout);
        self
    }

    /// Sets the resilience policy governing each embedding provider call.
    ///
    /// Routes provider calls through the toolkit's canonical [`rskit_resilience::Policy`] rather than a bespoke timeout, so timeouts, bounded retries, and circuit-breaking share one configurable seam. The default is a per-call 30-second timeout with no retries; embedding is idempotent, so a caller may add bounded jittered retries.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Sets the number of samples embedded per provider call.
    ///
    /// Bounds the size of each embedding request. A value of `0` is invalid and is rejected as a typed [`ErrorCode::InvalidInput`] error when the metric is computed, rather than being silently coerced.
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets the extractor rendering a label to the text embedded for scoring.
    #[must_use]
    pub fn with_extractor(
        mut self,
        extract: impl Fn(&L) -> String + Send + Sync + 'static,
    ) -> Self {
        self.extract = Arc::new(extract);
        self
    }

    fn zeroed_result(&self) -> MetricResult {
        self.result(0.0, 0.0)
    }

    fn result(&self, avg_similarity: f64, match_rate: f64) -> MetricResult {
        let mut values = HashMap::new();
        values.insert("avg_similarity".to_string(), avg_similarity);
        values.insert("match_rate".to_string(), match_rate);
        values.insert("threshold".to_string(), f64::from(self.threshold));
        MetricResult {
            name: self.name.clone(),
            value: avg_similarity,
            direction: MetricDirection::HigherIsBetter,
            values,
            detail: Some(self.provenance()),
        }
    }

    /// Records the embedding model's identity so a persisted result carries the scoring provenance, not just the similarity numbers.
    fn provenance(&self) -> serde_json::Value {
        serde_json::json!({ "model": self.model_id })
    }

    /// Embeds one batch of samples and returns the ordered embeddings, two per sample (reference then prediction), under a single bounded provider call.
    async fn embed_batch(&self, batch: &[ScoredSample<L>]) -> AppResult<Vec<Vec<f32>>> {
        // Reference and prediction for each sample are interleaved so pair `i` is at inputs `2i` (reference) and `2i + 1` (prediction).
        let mut inputs = Vec::with_capacity(batch.len() * 2);
        for sample in batch {
            inputs.push(EmbedInput::Text((self.extract)(&sample.sample.label)));
            inputs.push(EmbedInput::Text((self.extract)(&sample.prediction.label)));
        }

        let request = EmbedRequest {
            model: self.model.clone(),
            inputs,
            options: EmbeddingOptions::default(),
        };

        // Route every provider call through the injected resilience policy (default: a per-call timeout) rather than a bespoke timeout, so async metrics share the toolkit's canonical resilience seam. The request is cloned per attempt so a retrying policy re-issues an identical call.
        let provider = Arc::clone(&self.provider);
        let response = self
            .policy
            .execute(|| {
                let provider = Arc::clone(&provider);
                let request = request.clone();
                async move { provider.embed(request).await }
            })
            .await?;

        let expected = batch.len() * 2;
        if response.embeddings.len() != expected {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!(
                    "semantic_similarity: expected {expected} embeddings, provider returned {}",
                    response.embeddings.len()
                ),
            ));
        }

        // Provider output is untrusted: a correct-length response may still carry duplicate, missing, or out-of-range indices (for example `[0, 0]`), which would silently mismap references to predictions. Sort by the reported index, then require the result to be exactly the permutation `0..expected` before trusting the pairing.
        let mut embeddings = response.embeddings;
        embeddings.sort_by_key(|embedding| embedding.index);
        for (position, embedding) in embeddings.iter().enumerate() {
            if embedding.index != position {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "semantic_similarity: provider returned invalid embedding indices; \
                         expected a permutation of 0..{expected}"
                    ),
                ));
            }
            // A zero-value or dimension-inconsistent embedding (for example an entry an adapter preallocated for a missing response item) would otherwise be scored as a spurious similarity, so reject it as a malformed untrusted response rather than trusting its vector.
            if embedding.vector.is_empty() || embedding.dimensions != embedding.vector.len() {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "semantic_similarity: embedding at index {position} has an empty or \
                         dimension-inconsistent vector (dimensions {}, length {})",
                        embedding.dimensions,
                        embedding.vector.len()
                    ),
                ));
            }
        }
        Ok(embeddings.into_iter().map(|e| e.vector).collect())
    }
}

#[async_trait]
impl<L> AsyncMetric<L> for SemanticSimilarity<L>
where
    L: Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    async fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        if scored.is_empty() {
            return Ok(self.zeroed_result());
        }

        if self.batch_size == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "semantic_similarity: batch_size must be greater than zero",
            ));
        }
        let batch_size = self.batch_size;
        let mut total_similarity = 0.0_f64;
        let mut matches = 0_usize;
        for batch in scored.chunks(batch_size) {
            let vectors = self.embed_batch(batch).await?;
            for pair in vectors.chunks_exact(2) {
                let (reference, prediction) = (&pair[0], &pair[1]);
                if reference.len() != prediction.len() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "semantic_similarity: embedding dimension mismatch (reference {} vs prediction {})",
                            reference.len(),
                            prediction.len()
                        ),
                    ));
                }
                let similarity = f64::from(cosine_similarity(reference, prediction));
                total_similarity += similarity;
                if similarity >= f64::from(self.threshold) {
                    matches += 1;
                }
            }
        }

        let count = scored.len() as f64;
        Ok(self.result(total_similarity / count, matches as f64 / count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BenchSample, Prediction};
    use rskit_ai::{Capabilities, Provider as ModelProvider};
    use rskit_embedding::InMemoryProvider;
    use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
    use rskit_testutil::FakeEmbeddingProvider;

    fn model() -> Model {
        Model {
            name: "embed-test".into(),
            provider: ModelProvider::Custom("memory".into()),
            version: None,
            capabilities: Capabilities::default(),
        }
    }

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

    #[tokio::test]
    async fn identical_vectors_score_full_similarity() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![vec![1.0, 0.0], vec![1.0, 0.0]]);
        let metric = semantic_similarity::<String>(provider, model());
        let result = metric.compute(&[scored("a", "a")]).await.expect("compute");
        assert!((result.value - 1.0).abs() < 1e-6);
        assert_eq!(result.values["match_rate"], 1.0);
        assert_eq!(result.direction, MetricDirection::HigherIsBetter);
    }

    #[tokio::test]
    async fn orthogonal_vectors_score_zero_and_miss_threshold() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        let metric = semantic_similarity::<String>(provider, model());
        let result = metric.compute(&[scored("a", "b")]).await.expect("compute");
        assert!(result.value.abs() < 1e-6);
        assert_eq!(result.values["match_rate"], 0.0);
    }

    #[tokio::test]
    async fn threshold_controls_match_rate() {
        // Two pairs: one identical (sim 1.0), one orthogonal (sim 0.0).
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![
            vec![1.0, 0.0],
            vec![1.0, 0.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
        ]);
        let metric = semantic_similarity::<String>(provider, model()).with_threshold(0.5);
        let result = metric
            .compute(&[scored("a", "a"), scored("b", "c")])
            .await
            .expect("compute");
        assert!((result.values["avg_similarity"] - 0.5).abs() < 1e-6);
        assert_eq!(result.values["match_rate"], 0.5);
        assert_eq!(result.values["threshold"], 0.5);
    }

    #[tokio::test]
    async fn empty_input_is_zeroed_without_calling_provider() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        let metric =
            semantic_similarity::<String>(Arc::clone(&provider) as Arc<dyn Provider>, model());
        let result = metric.compute(&[]).await.expect("compute");
        assert_eq!(result.value, 0.0);
        assert_eq!(result.values["match_rate"], 0.0);
        assert_eq!(provider.call_count(), 0);
    }

    #[tokio::test]
    async fn provider_error_surfaces_as_typed_error() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_fail(AppError::new(ErrorCode::ServiceUnavailable, "embed down"));
        let metric = semantic_similarity::<String>(provider, model());
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("provider error must surface");
        assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    }

    #[tokio::test]
    async fn dimension_mismatch_is_typed_error_not_panic() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        // Reference has 3 dims, prediction has 2 — an incoherent provider result.
        provider.will_return(vec![vec![1.0, 0.0, 0.0], vec![1.0, 0.0]]);
        let metric = semantic_similarity::<String>(provider, model());
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("dimension mismatch must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("dimension mismatch"));
    }

    #[tokio::test]
    async fn provider_timeout_is_typed_error() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_hang();
        // A zero timeout elapses before the hung provider can resolve.
        let metric = semantic_similarity::<String>(provider, model()).with_timeout(Duration::ZERO);
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("timeout must error");
        assert_eq!(err.code(), ErrorCode::Timeout);
    }

    #[tokio::test]
    async fn deterministic_with_a_fixed_provider() {
        // InMemoryProvider is stateless and deterministic, so repeated scoring of the same samples yields an identical result.
        let metric = semantic_similarity::<String>(Arc::new(InMemoryProvider::new(8)), model());
        let samples = [scored("hello", "hallo"), scored("world", "word")];
        let first = metric.compute(&samples).await.expect("compute");
        let second = metric.compute(&samples).await.expect("compute again");
        assert_eq!(first.value, second.value);
        assert_eq!(first.values["match_rate"], second.values["match_rate"]);
    }

    #[tokio::test]
    async fn extractor_selects_embedded_text() {
        // A constant extractor makes reference and prediction embed identical text regardless of label, forcing a perfect similarity.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![vec![0.5, 0.5], vec![0.5, 0.5]]);
        let metric = semantic_similarity::<String>(provider, model())
            .with_extractor(|_label| "constant".to_string());
        let result = metric.compute(&[scored("x", "y")]).await.expect("compute");
        assert!((result.value - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn bounds_requests_into_multiple_batches() {
        // Three samples with batch_size 2 embed in two provider calls (2 samples, then 1). Each call is scripted separately, and the pooled result matches scoring them together.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider
            .will_return(vec![
                vec![1.0, 0.0],
                vec![1.0, 0.0],
                vec![1.0, 0.0],
                vec![0.0, 1.0],
            ])
            .will_return(vec![vec![1.0, 0.0], vec![1.0, 0.0]]);
        let metric =
            semantic_similarity::<String>(Arc::clone(&provider) as Arc<dyn Provider>, model())
                .with_batch_size(2)
                .with_threshold(0.5);
        let result = metric
            .compute(&[scored("a", "a"), scored("b", "c"), scored("d", "d")])
            .await
            .expect("compute");
        // Similarities: 1.0, 0.0, 1.0 -> avg 2/3, match_rate 2/3.
        assert!((result.values["avg_similarity"] - 2.0 / 3.0).abs() < 1e-6);
        assert!((result.values["match_rate"] - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn zero_batch_size_is_rejected() {
        // An invalid batch size is a typed error, not a silent coercion, and is rejected before any provider call.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        let metric =
            semantic_similarity::<String>(Arc::clone(&provider) as Arc<dyn Provider>, model())
                .with_batch_size(0);
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("zero batch size must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert_eq!(provider.call_count(), 0);
    }

    #[tokio::test]
    async fn metric_name_embeds_model_identity() {
        // The name carries the model identity so runs scored with incompatible embedding models never collide under a bare "semantic_similarity" name.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        let metric = semantic_similarity::<String>(provider, model());
        assert_eq!(metric.name(), "semantic_similarity[memory/embed-test]");
    }

    #[tokio::test]
    async fn distinct_models_produce_distinct_names() {
        // Two models with different identities must not share a metric name, so run comparison never treats them as the same metric.
        let a = semantic_similarity::<String>(Arc::new(FakeEmbeddingProvider::new()), model());
        let mut other = model();
        other.name = "embed-large".into();
        let b = semantic_similarity::<String>(Arc::new(FakeEmbeddingProvider::new()), other);
        assert_ne!(a.name(), b.name());
    }

    #[tokio::test]
    async fn model_identity_is_collision_free_across_delimiters() {
        // Two distinct (provider, name) pairs that would collapse to the same `a/b/c` string under a naive join must produce distinct identities once each component is escaped.
        let mut left = model();
        left.provider = ModelProvider::Custom("a/b".into());
        left.name = "c".into();
        let mut right = model();
        right.provider = ModelProvider::Custom("a".into());
        right.name = "b/c".into();
        assert_ne!(model_identity(&left), model_identity(&right));
    }

    #[tokio::test]
    async fn result_records_model_provenance() {
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![vec![1.0, 0.0], vec![1.0, 0.0]]);
        let metric = semantic_similarity::<String>(provider, model());
        let result = metric.compute(&[scored("a", "a")]).await.expect("compute");
        assert_eq!(result.name, "semantic_similarity[memory/embed-test]");
        let detail = result.detail.expect("provenance detail present");
        assert_eq!(detail["model"], "memory/embed-test");
    }

    #[tokio::test]
    async fn provider_returned_invalid_indices_are_rejected() {
        // A correct-length response whose indices are not a `0..n` permutation (here `[0, 0]`) must be rejected, not silently mismapped.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return_indexed(vec![(vec![1.0, 0.0], 0), (vec![0.0, 1.0], 0)]);
        let metric = semantic_similarity::<String>(provider, model());
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("invalid indices must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn provider_returned_empty_vector_is_rejected() {
        // A provider (or an adapter preallocating for a missing item) may return a zero-value, empty-vector embedding. Accepting it would score a spurious similarity, so a correct-length, correctly-indexed response with an empty vector must still be rejected.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider.will_return(vec![Vec::new(), vec![1.0, 0.0]]);
        let metric = semantic_similarity::<String>(provider, model());
        let err = metric
            .compute(&[scored("a", "a")])
            .await
            .expect_err("empty-vector embedding must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn injected_policy_retries_recoverable_provider_error() {
        // The first call fails with a retryable error; the injected retry policy re-issues it and the second call succeeds — proving provider calls flow through the canonical resilience seam rather than a bespoke timeout.
        let provider = Arc::new(FakeEmbeddingProvider::new());
        provider
            .will_fail(AppError::new(ErrorCode::ServiceUnavailable, "transient"))
            .will_return(vec![vec![1.0, 0.0], vec![1.0, 0.0]]);
        let policy = Policy::new().with_retry(
            RetryPolicy::new()
                .with_max_attempts(2)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                .with_jitter(false),
        );
        let metric =
            semantic_similarity::<String>(Arc::clone(&provider) as Arc<dyn Provider>, model())
                .with_policy(policy);
        let result = metric.compute(&[scored("a", "a")]).await.expect("compute");
        assert!((result.value - 1.0).abs() < 1e-6);
        assert_eq!(provider.call_count(), 2);
    }
}

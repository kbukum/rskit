//! Scriptable embedding provider fake for tests.
//!
//! [`FakeEmbeddingProvider`] returns caller-scripted vectors instead of calling a real embedding backend, so metric and pipeline tests stay deterministic and offline. Enqueue vectors to control the similarity a metric will compute, an error to exercise provider-failure paths, or vectors of differing lengths to exercise dimension-mismatch handling.

use std::collections::VecDeque;

use async_trait::async_trait;
use parking_lot::Mutex;

use rskit_embedding::{EmbedRequest, EmbedResponse, Embedding, Provider, Usage};
use rskit_errors::{AppError, AppResult, ErrorCode};

/// A scripted response for one [`FakeEmbeddingProvider::embed`] call.
enum Script {
    /// Return these vectors as embeddings, in input order.
    Return(Vec<Vec<f32>>),
    /// Return these vectors as embeddings with explicit, caller-supplied indices (which may be duplicate, missing, or out of range).
    ReturnIndexed(Vec<(Vec<f32>, usize)>),
    /// Return this error.
    Fail(AppError),
    /// Never resolve, so the caller's timeout/cancellation path is exercised.
    Hang,
}

/// A scriptable [`Provider`] fake that returns pre-configured embedding vectors.
///
/// Each enqueued script drives the next [`embed`](Provider::embed) call: [`will_return`](Self::will_return) yields vectors as [`Embedding`]s in input order (use differing lengths to drive dimension-mismatch handling), [`will_fail`](Self::will_fail) drives provider-failure paths, and [`will_hang`](Self::will_hang) never resolves so a caller's timeout and cancellation path can be exercised. The fake performs no network or model I/O and is fully deterministic.
pub struct FakeEmbeddingProvider {
    scripts: Mutex<VecDeque<Script>>,
    calls: Mutex<usize>,
}

impl FakeEmbeddingProvider {
    /// Creates a fake with no scripted responses.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Mutex::new(VecDeque::new()),
            calls: Mutex::new(0),
        }
    }

    /// Enqueues the vectors returned by the next `embed` call, in input order.
    pub fn will_return(&self, vectors: Vec<Vec<f32>>) -> &Self {
        self.scripts.lock().push_back(Script::Return(vectors));
        self
    }

    /// Enqueues explicitly-indexed embeddings for the next `embed` call.
    ///
    /// Unlike [`will_return`](Self::will_return), which assigns sequential input indices, this lets a test provide duplicate, missing, or out-of-range indices to exercise a caller's validation of untrusted provider output.
    pub fn will_return_indexed(&self, indexed: Vec<(Vec<f32>, usize)>) -> &Self {
        self.scripts
            .lock()
            .push_back(Script::ReturnIndexed(indexed));
        self
    }

    /// Enqueues an error returned by the next `embed` call.
    pub fn will_fail(&self, err: AppError) -> &Self {
        self.scripts.lock().push_back(Script::Fail(err));
        self
    }

    /// Enqueues an `embed` call that never resolves, to exercise timeout paths.
    pub fn will_hang(&self) -> &Self {
        self.scripts.lock().push_back(Script::Hang);
        self
    }

    /// Returns how many `embed` calls have been recorded.
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.calls.lock()
    }

    fn next_script(&self) -> Script {
        *self.calls.lock() += 1;
        self.scripts.lock().pop_front().unwrap_or_else(|| {
            Script::Fail(AppError::new(
                ErrorCode::Internal,
                "FakeEmbeddingProvider: no scripted response enqueued",
            ))
        })
    }
}

impl Default for FakeEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for FakeEmbeddingProvider {
    async fn embed(&self, req: EmbedRequest) -> AppResult<EmbedResponse> {
        match self.next_script() {
            Script::Return(vectors) => {
                let embeddings = vectors
                    .into_iter()
                    .enumerate()
                    .map(|(index, vector)| Embedding::new(vector, index))
                    .collect();
                Ok(EmbedResponse {
                    embeddings,
                    model: req.model,
                    usage: Usage::default(),
                })
            }
            Script::ReturnIndexed(indexed) => {
                let embeddings = indexed
                    .into_iter()
                    .map(|(vector, index)| Embedding::new(vector, index))
                    .collect();
                Ok(EmbedResponse {
                    embeddings,
                    model: req.model,
                    usage: Usage::default(),
                })
            }
            Script::Fail(err) => Err(err),
            Script::Hang => std::future::pending().await,
        }
    }

    async fn embed_batch(&self, reqs: Vec<EmbedRequest>) -> AppResult<Vec<EmbedResponse>> {
        let mut responses = Vec::with_capacity(reqs.len());
        for req in reqs {
            responses.push(self.embed(req).await?);
        }
        Ok(responses)
    }
}

impl rskit_provider::Provider for FakeEmbeddingProvider {
    fn name(&self) -> &'static str {
        "fake_embedding"
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<EmbedRequest, EmbedResponse> for FakeEmbeddingProvider {
    async fn execute(&self, input: EmbedRequest) -> AppResult<EmbedResponse> {
        self.embed(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_ai::{Capabilities, Model, Provider as ModelProvider};
    use rskit_embedding::{EmbedInput, EmbeddingOptions};

    fn request(inputs: &[&str]) -> EmbedRequest {
        EmbedRequest {
            model: Model {
                name: "fake".into(),
                provider: ModelProvider::Custom("fake".into()),
                version: None,
                capabilities: Capabilities::default(),
            },
            inputs: inputs
                .iter()
                .map(|text| EmbedInput::Text((*text).to_string()))
                .collect(),
            options: EmbeddingOptions::default(),
        }
    }

    #[tokio::test]
    async fn returns_scripted_vectors_in_order() {
        let provider = FakeEmbeddingProvider::new();
        provider.will_return(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        let response = provider.embed(request(&["a", "b"])).await.expect("embed");
        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0].vector, vec![1.0, 0.0]);
        assert_eq!(response.embeddings[1].vector, vec![0.0, 1.0]);
        assert_eq!(response.embeddings[1].index, 1);
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn indexed_script_preserves_supplied_indices() {
        // Explicit indices let a test drive a caller's validation of untrusted provider output — here two embeddings both claim index 0.
        let provider = FakeEmbeddingProvider::new();
        provider.will_return_indexed(vec![(vec![1.0], 0), (vec![2.0], 0)]);
        let response = provider.embed(request(&["a", "b"])).await.expect("embed");
        assert_eq!(response.embeddings[0].index, 0);
        assert_eq!(response.embeddings[1].index, 0);
    }

    #[tokio::test]
    async fn scripted_error_is_surfaced() {
        let provider = FakeEmbeddingProvider::new();
        provider.will_fail(AppError::new(ErrorCode::ServiceUnavailable, "embed down"));
        let err = provider
            .embed(request(&["a"]))
            .await
            .expect_err("scripted error must surface");
        assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    }

    #[tokio::test]
    async fn missing_script_errors_instead_of_panicking() {
        let provider = FakeEmbeddingProvider::new();
        let err = provider
            .embed(request(&["a"]))
            .await
            .expect_err("missing script must error");
        assert_eq!(err.code(), ErrorCode::Internal);
    }

    #[tokio::test]
    async fn batch_returns_one_response_per_request() {
        let provider = FakeEmbeddingProvider::new();
        provider
            .will_return(vec![vec![1.0]])
            .will_return(vec![vec![2.0]]);
        let responses = provider
            .embed_batch(vec![request(&["a"]), request(&["b"])])
            .await
            .expect("batch");
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[1].embeddings[0].vector, vec![2.0]);
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn hanging_call_never_resolves_within_timeout() {
        let provider = FakeEmbeddingProvider::new();
        provider.will_hang();
        let elapsed =
            tokio::time::timeout(std::time::Duration::ZERO, provider.embed(request(&["a"]))).await;
        assert!(elapsed.is_err(), "hanging call must not resolve");
    }
}

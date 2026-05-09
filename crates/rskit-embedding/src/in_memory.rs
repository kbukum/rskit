//! Deterministic in-memory embedding adapter for tests.

use async_trait::async_trait;
use rskit_ai::Usage;
use rskit_errors::AppResult;

use crate::{EmbedInput, EmbedRequest, EmbedResponse, Embedding, Provider};

/// Deterministic embedding provider for tests and examples.
#[derive(Debug, Clone)]
pub struct InMemoryProvider {
    dimensions: usize,
}

impl InMemoryProvider {
    /// Create a deterministic provider with fixed vector dimensions.
    #[must_use]
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }

    fn vector_for(&self, input: &EmbedInput) -> Vec<f32> {
        let bytes: Vec<u8> = match input {
            EmbedInput::Text(text) => text.as_bytes().to_vec(),
            EmbedInput::Image(asset) | EmbedInput::Audio(asset) | EmbedInput::Video(asset) => {
                serde_json::to_vec(asset).unwrap_or_default()
            }
        };
        (0..self.dimensions)
            .map(|idx| {
                let sum = bytes
                    .iter()
                    .enumerate()
                    .fold(idx as u32, |acc, (pos, byte)| {
                        acc.wrapping_add(u32::from(*byte) * ((pos + idx + 1) as u32))
                    });
                (sum % 1000) as f32 / 1000.0
            })
            .collect()
    }
}

impl Default for InMemoryProvider {
    fn default() -> Self {
        Self::new(8)
    }
}

#[async_trait]
impl Provider for InMemoryProvider {
    async fn embed(&self, req: EmbedRequest) -> AppResult<EmbedResponse> {
        let embeddings = req
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| Embedding::new(self.vector_for(input), index))
            .collect();
        Ok(EmbedResponse {
            embeddings,
            model: req.model,
            usage: Usage::default(),
        })
    }

    async fn embed_batch(&self, reqs: Vec<EmbedRequest>) -> AppResult<Vec<EmbedResponse>> {
        let mut responses = Vec::with_capacity(reqs.len());
        for req in reqs {
            responses.push(self.embed(req).await?);
        }
        Ok(responses)
    }
}

impl rskit_provider::Provider for InMemoryProvider {
    fn name(&self) -> &'static str {
        "in_memory_embedding"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_ai::{Capabilities, Model, Provider as ModelProvider};

    fn model() -> Model {
        Model {
            name: "embed-test".into(),
            provider: ModelProvider::Custom("memory".into()),
            version: None,
            capabilities: Capabilities::default(),
        }
    }

    #[tokio::test]
    async fn deterministic_adapter_embeds_inputs() {
        let provider = InMemoryProvider::new(4);
        let req = EmbedRequest {
            model: model(),
            inputs: vec![
                EmbedInput::Text("hello".into()),
                EmbedInput::Text("world".into()),
            ],
            options: serde_json::Value::Null,
        };
        let response = provider.embed(req.clone()).await.expect("embed");
        let again = provider.embed(req).await.expect("embed again");
        assert_eq!(response.embeddings, again.embeddings);
        assert_eq!(response.embeddings[0].dimensions, 4);
        assert_eq!(response.embeddings[1].index, 1);
        assert_eq!(response.usage, Usage::default());
    }

    #[tokio::test]
    async fn batch_returns_one_response_per_request() {
        let provider = InMemoryProvider::default();
        let req = EmbedRequest {
            model: model(),
            inputs: vec![EmbedInput::Text("x".into())],
            options: serde_json::json!({}),
        };
        let responses = provider
            .embed_batch(vec![req.clone(), req])
            .await
            .expect("batch");
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].embeddings[0].dimensions, 8);
    }
}

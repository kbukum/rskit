//! OpenAI-compatible embedding provider.
//!
//! Works with OpenAI, Azure OpenAI, local llama.cpp, vLLM, or any server
//! that exposes the `/v1/embeddings` endpoint.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::provider::EmbeddingProvider;

/// Configuration for the OpenAI-compatible embedding provider.
#[derive(Debug, Clone)]
pub struct OpenAiEmbeddingConfig {
    /// Base URL for the API (e.g., `https://api.openai.com`).
    pub endpoint: String,
    /// API key for authentication. Empty string disables the header.
    pub api_key: String,
    /// Model name (e.g., `text-embedding-3-small`).
    pub model: String,
    /// Expected embedding dimensions.
    pub dimensions: usize,
}

impl Default for OpenAiEmbeddingConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com".to_owned(),
            api_key: String::new(),
            model: "text-embedding-3-small".to_owned(),
            dimensions: 1536,
        }
    }
}

/// OpenAI-compatible embedding provider.
pub struct OpenAiEmbeddingProvider {
    client: reqwest::Client,
    config: OpenAiEmbeddingConfig,
}

impl OpenAiEmbeddingProvider {
    /// Create a new OpenAI embedding provider with the given configuration.
    pub fn new(config: OpenAiEmbeddingConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddingProvider {
    async fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        let results = self.embed_batch(&[text]).await?;
        results.into_iter().next().ok_or_else(|| {
            AppError::new(
                ErrorCode::ExternalService,
                "empty embedding response from OpenAI",
            )
        })
    }

    async fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/v1/embeddings",
            self.config.endpoint.trim_end_matches('/')
        );
        let body = EmbeddingRequest {
            model: self.config.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        debug!(
            model = %self.config.model,
            count = texts.len(),
            "Requesting embeddings"
        );

        let mut req = self.client.post(&url).json(&body);
        if !self.config.api_key.is_empty() {
            req = req.bearer_auth(&self.config.api_key);
        }

        let resp = req.send().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("embedding request failed: {e}"),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AppError::new(
                ErrorCode::ExternalService,
                format!("embedding API returned HTTP {status}: {body_text}"),
            ));
        }

        let result: EmbeddingResponse = resp.json().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse embedding response: {e}"),
            )
        })?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.config.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = OpenAiEmbeddingConfig::default();
        assert_eq!(cfg.endpoint, "https://api.openai.com");
        assert_eq!(cfg.model, "text-embedding-3-small");
        assert_eq!(cfg.dimensions, 1536);
    }
}

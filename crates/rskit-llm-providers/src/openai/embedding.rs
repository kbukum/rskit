//! OpenAI-compatible embedding provider using rskit-httpclient.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::config::Config;

/// OpenAI-compatible embedding provider backed by rskit-httpclient.
pub struct EmbeddingProvider {
    client: HttpClient,
    model: String,
    dimensions: usize,
}

impl EmbeddingProvider {
    /// Create a new embedding provider from an OpenAI [`Config`].
    pub fn new(cfg: &Config) -> AppResult<Self> {
        let http_cfg = HttpClientConfig::new()
            .with_base_url(&cfg.base_url)
            .with_auth(Auth::bearer(&cfg.api_key));

        let client = HttpClient::new(http_cfg)?;

        Ok(Self {
            client,
            model: cfg.embedding_model.clone(),
            dimensions: cfg.embedding_dimensions,
        })
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
impl rskit_embedding::EmbeddingProvider for EmbeddingProvider {
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

        let body = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        debug!(model = %self.model, count = texts.len(), "requesting embeddings");

        let request = Request::post("/embeddings").json_body(&body).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to build embedding request: {e}"),
            )
        })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(AppError::new(
                ErrorCode::ExternalService,
                format!("embedding API returned HTTP {status}: {text}"),
            ));
        }

        let result: EmbeddingResponse = response.json().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse embedding response: {e}"),
            )
        })?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_constructs_with_config() {
        use rskit_embedding::EmbeddingProvider as _;

        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let provider = EmbeddingProvider::new(&cfg).unwrap();
        assert_eq!(provider.dimensions(), 1536);
    }
}

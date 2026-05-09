//! OpenAI-compatible embedding provider using rskit-httpclient.

use async_trait::async_trait;
use rskit_embedding::{EmbedInput, EmbedRequest, EmbedResponse, Embedding, Provider};
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

    /// Return the configured embedding dimensionality.
    #[must_use]
    pub const fn dimensions(&self) -> usize {
        self.dimensions
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
impl Provider for EmbeddingProvider {
    async fn embed(&self, req: EmbedRequest) -> AppResult<EmbedResponse> {
        let mut response_model = req.model.clone();
        if response_model.name.is_empty() {
            response_model.name.clone_from(&self.model);
        }
        let model = response_model.name.clone();
        let texts = req
            .inputs
            .iter()
            .map(|input| match input {
                EmbedInput::Text(text) => Ok(text.clone()),
                _ => Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "OpenAI embedding adapter currently accepts text inputs only",
                )),
            })
            .collect::<AppResult<Vec<_>>>()?;

        if texts.is_empty() {
            return Ok(EmbedResponse {
                embeddings: Vec::new(),
                model: response_model,
                usage: rskit_ai::Usage::default(),
            });
        }

        let body = EmbeddingRequest {
            model: model.clone(),
            input: texts,
        };

        debug!(model = %model, count = body.input.len(), "requesting embeddings");

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

        Ok(EmbedResponse {
            embeddings: result
                .data
                .into_iter()
                .enumerate()
                .map(|(index, data)| Embedding::new(data.embedding, index))
                .collect(),
            model: response_model,
            usage: rskit_ai::Usage::default(),
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

impl rskit_provider::Provider for EmbeddingProvider {
    fn name(&self) -> &'static str {
        "openai_embedding"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_constructs_with_config() {
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

//! OpenAI-compatible embedding provider using rskit-httpclient.

use async_trait::async_trait;
use rskit_ai::semconv;
use rskit_embedding::{EmbedInput, EmbedRequest, EmbedResponse, Embedding, Provider};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_resilience::Policy;
use serde::{Deserialize, Serialize};
use tracing::{Instrument, debug};

use super::config::Config;

/// OpenAI-compatible embedding provider backed by rskit-httpclient.
pub struct EmbeddingProvider {
    client: HttpClient,
    model: String,
    dimensions: usize,
    policy: Option<Policy>,
}

impl EmbeddingProvider {
    /// Create a new embedding provider from an `OpenAI` [`Config`].
    pub(super) fn new(cfg: &Config) -> AppResult<Self> {
        let http_cfg = HttpClientConfig::new()
            .with_base_url(&cfg.base_url)
            .with_auth(Auth::bearer(&cfg.api_key));

        let client = HttpClient::new(http_cfg)?;

        Ok(Self {
            client,
            model: cfg.embedding_model.clone(),
            dimensions: cfg.embedding_dimensions,
            policy: None,
        })
    }

    /// Inject a resilience policy for outbound embedding requests.
    #[must_use]
    pub(super) fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[serde(default)]
    usage: Option<EmbeddingUsage>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingUsage {
    prompt_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[async_trait]
impl Provider for EmbeddingProvider {
    async fn embed(&self, req: EmbedRequest) -> AppResult<EmbedResponse> {
        let mut response_model = req.model.clone();
        if response_model.name.is_empty() {
            response_model.name.clone_from(&self.model);
        }
        let model = response_model.name.clone();

        let span = tracing::info_span!(
            "embedding.embed",
            "gen_ai.system" = "openai",
            "gen_ai.operation.name" = semconv::Operation::Embedding.as_str(),
            "gen_ai.request.model" = %model,
            "embedding.input_count" = req.inputs.len(),
        );
        async move {
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
                dimensions: self.dimensions,
            };

            debug!(model = %model, count = body.input.len(), "requesting embeddings");

            let request = Request::post("/embeddings")
                .json_body(&body)
                .map_err(|e| AppError::internal(e).context("build embedding request"))?;

            let policy = self.policy.clone();
            let response = if let Some(policy) = policy {
                let request = request.clone();
                policy
                    .execute(|| {
                        let request = request.clone();
                        async move {
                            let resp = self.client.send(request).await?;
                            if !resp.is_success() {
                                let status = resp.status().as_u16();
                                let text = resp.text().unwrap_or_default();
                                return Err(AppError::new(
                                    ErrorCode::ExternalService,
                                    format!("embedding API returned HTTP {status}"),
                                )
                                .with_detail("status", status.to_string())
                                .with_detail("body", text));
                            }
                            Ok(resp)
                        }
                    })
                    .await?
            } else {
                let resp = self.client.send(request).await?;
                if !resp.is_success() {
                    let status = resp.status().as_u16();
                    let text = resp.text().unwrap_or_default();
                    return Err(AppError::new(
                        ErrorCode::ExternalService,
                        format!("embedding API returned HTTP {status}"),
                    )
                    .with_detail("status", status.to_string())
                    .with_detail("body", text));
                }
                resp
            };

            let result: EmbeddingResponse = response
                .json()
                .map_err(|e| AppError::internal(e).context("parse embedding response"))?;

            let usage = result
                .usage
                .map(|u| rskit_ai::Usage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.total_tokens.saturating_sub(u.prompt_tokens),
                    ..Default::default()
                })
                .unwrap_or_default();

            Ok(EmbedResponse {
                embeddings: result
                    .data
                    .into_iter()
                    .enumerate()
                    .map(|(index, data)| Embedding::new(data.embedding, index))
                    .collect(),
                model: response_model,
                usage,
            })
        }
        .instrument(span)
        .await
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

#[async_trait]
impl rskit_provider::RequestResponse<EmbedRequest, EmbedResponse> for EmbeddingProvider {
    async fn execute(&self, input: EmbedRequest) -> AppResult<EmbedResponse> {
        self.embed(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_resilience::{ConstantBackoff, RetryPolicy};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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
        assert_eq!(provider.dimensions, 1536);
    }

    #[tokio::test]
    async fn provider_retries_with_policy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_in_server = attempts.clone();

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let attempts = attempts_in_server.clone();
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        socket
                            .write_all(
                                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 12\r\nconnection: close\r\n\r\nretry later",
                            )
                            .await
                            .unwrap();
                    } else {
                        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}],"usage":{"prompt_tokens":2,"total_tokens":2}}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        socket.write_all(response.as_bytes()).await.unwrap();
                    }
                    socket.shutdown().await.unwrap();
                });
            }
        });

        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: format!("http://{address}"),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 3,
        };
        let provider = EmbeddingProvider::new(&cfg).unwrap().with_policy(
            Policy::new().with_retry(
                RetryPolicy::new()
                    .with_max_attempts(2)
                    .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                    .with_jitter(false),
            ),
        );
        let response = provider
            .embed(EmbedRequest {
                model: rskit_ai::Model {
                    name: String::new(),
                    provider: rskit_ai::Provider::OpenAI,
                    version: None,
                    capabilities: rskit_ai::Capabilities::default(),
                },
                inputs: vec![EmbedInput::Text("retry".into())],
                options: rskit_embedding::EmbeddingOptions::default(),
            })
            .await
            .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(response.embeddings.len(), 1);
        server.await.unwrap();
    }
}

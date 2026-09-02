//! OpenAI-compatible embedding provider using rskit-httpclient.

use async_trait::async_trait;
use rskit_ai::semconv;
use rskit_embedding::{EmbedInput, EmbedRequest, EmbedResponse, Embedding, Provider};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_observability::set_span_attribute;
use rskit_resilience::Policy;
use serde::{Deserialize, Serialize};
use tracing::{Instrument, debug};

use super::PROVIDER_ID;
use super::config::Config;

/// OpenAI-compatible embedding provider backed by rskit-httpclient.
pub struct EmbeddingProvider {
    client: HttpClient,
    model: String,
    dimensions: Option<usize>,
    policy: Option<Policy>,
}

impl EmbeddingProvider {
    /// Create a new embedding provider from an `OpenAI` [`Config`].
    pub fn new(cfg: &Config) -> AppResult<Self> {
        let http_cfg = cfg.transport.apply_to(
            HttpClientConfig::new()
                .with_base_url(&cfg.base_url)
                .with_auth(Auth::bearer_secret(cfg.api_key.clone())),
        );

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
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }
}

/// Build an OpenAI-compatible embedding provider from adapter configuration.
pub fn embedding_provider(config: &Config) -> AppResult<EmbeddingProvider> {
    EmbeddingProvider::new(config)
}

/// Build an OpenAI-compatible embedding provider with a resilience policy.
pub fn embedding_provider_with_policy(
    config: &Config,
    policy: rskit_resilience::Policy,
) -> AppResult<EmbeddingProvider> {
    Ok(EmbeddingProvider::new(config)?.with_policy(policy))
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
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

        let span = embedding_span(&model, req.inputs.len());
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
                                let status = resp.status_u16();
                                let text = resp.text_or_diagnostic();
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
                    let status = resp.status_u16();
                    let text = resp.text_or_diagnostic();
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

fn embedding_span(model: &str, input_count: usize) -> tracing::Span {
    let span = tracing::info_span!(
        "embedding.embed",
        "gen_ai.system" = PROVIDER_ID,
        "gen_ai.operation.name" = semconv::Operation::Embedding.as_str(),
        "gen_ai.request.model" = %model,
        "embedding.input_count" = input_count,
    );
    set_span_attribute(&span, semconv::SYSTEM, PROVIDER_ID);
    set_span_attribute(
        &span,
        semconv::OPERATION_NAME,
        semconv::Operation::Embedding.as_str(),
    );
    set_span_attribute(&span, semconv::REQUEST_MODEL, model);
    span
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
    use rskit_embedding::EmbedAsset;
    use rskit_llm_common::HttpTransportConfig;
    use rskit_resilience::{ConstantBackoff, RetryPolicy};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn provider_constructs_with_config() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
            transport: HttpTransportConfig::default(),
        };
        let provider = EmbeddingProvider::new(&cfg).unwrap();
        assert_eq!(provider.dimensions, Some(1536));
    }

    #[test]
    fn embedding_request_omits_dimensions_when_unset() {
        let body = EmbeddingRequest {
            model: "text-embedding-ada-002".into(),
            input: vec!["hello".into()],
            dimensions: None,
        };

        let json = serde_json::to_value(body).unwrap();
        assert!(json.get("dimensions").is_none());
    }

    #[test]
    fn embedding_request_includes_dimensions_when_set() {
        let body = EmbeddingRequest {
            model: "text-embedding-3-small".into(),
            input: vec!["hello".into()],
            dimensions: Some(768),
        };

        let json = serde_json::to_value(body).unwrap();
        assert_eq!(json["dimensions"], 768);
    }

    #[tokio::test]
    async fn embed_returns_empty_response_without_http_for_empty_inputs() {
        let provider = EmbeddingProvider::new(&config(None)).unwrap();

        let response = provider.embed(request(Vec::new())).await.unwrap();

        assert!(response.embeddings.is_empty());
        assert_eq!(response.model.name, "text-embedding-3-small");
    }

    #[tokio::test]
    async fn embed_rejects_non_text_inputs_before_http() {
        let provider = EmbeddingProvider::new(&config(None)).unwrap();

        let err = provider
            .embed(request(vec![EmbedInput::Image(EmbedAsset::Url(
                "https://example.test/image.png".into(),
            ))]))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn embed_batch_and_request_response_forward_to_embed() {
        let (base_url, server) = spawn_response_server(vec![
            (
                200,
                r#"{"data":[{"embedding":[0.1]}],"usage":{"prompt_tokens":1,"total_tokens":2}}"#,
            ),
            (
                200,
                r#"{"data":[{"embedding":[0.2]}],"usage":{"prompt_tokens":2,"total_tokens":3}}"#,
            ),
        ])
        .await;
        let provider = EmbeddingProvider::new(&config(Some(base_url))).unwrap();

        assert_eq!(
            rskit_provider::Provider::name(&provider),
            "openai_embedding"
        );
        let via_trait = rskit_provider::RequestResponse::execute(
            &provider,
            request(vec![EmbedInput::Text("one".into())]),
        )
        .await
        .unwrap();
        let batch = provider
            .embed_batch(vec![request(vec![EmbedInput::Text("two".into())])])
            .await
            .unwrap();

        assert_eq!(via_trait.embeddings[0].vector, vec![0.1]);
        assert_eq!(batch[0].embeddings[0].vector, vec![0.2]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn embed_maps_http_errors_without_policy() {
        let (base_url, server) = spawn_response_server(vec![(503, "try later")]).await;
        let provider = EmbeddingProvider::new(&config(Some(base_url))).unwrap();

        let err = provider
            .embed(request(vec![EmbedInput::Text("hello".into())]))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::ExternalService);
        assert!(err.message().contains("embedding API returned HTTP 503"));
        server.await.unwrap();
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
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: format!("http://{address}"),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(3),
            transport: HttpTransportConfig::default(),
        };
        let provider = EmbeddingProvider::new(&cfg).unwrap().with_policy(
            Policy::new().with_retry(
                RetryPolicy::fast()
                    .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                    .with_jitter(0.0),
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

    fn config(base_url: Option<String>) -> Config {
        Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(3),
            transport: HttpTransportConfig::default(),
        }
    }

    fn request(inputs: Vec<EmbedInput>) -> EmbedRequest {
        EmbedRequest {
            model: rskit_ai::Model {
                name: String::new(),
                provider: rskit_ai::Provider::OpenAI,
                version: None,
                capabilities: rskit_ai::Capabilities::default(),
            },
            inputs,
            options: rskit_embedding::EmbeddingOptions::default(),
        }
    }

    async fn spawn_response_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    let reason = if status >= 400 { "Error" } else { "OK" };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.shutdown().await.unwrap();
                });
            }
        });
        (format!("http://{address}"), server)
    }
}

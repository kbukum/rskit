//! Adapter factory: bridges OpenAI [`Config`] → [`Provider`] via rskit-httpclient.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};
use rskit_resilience::Policy;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::config::Config;
use super::dialect::OpenAiDialect;

const SYSTEM: &str = "openai";

/// A [`Provider`] backed by the OpenAI chat-completions API.
pub struct OpenAiAdapter {
    client: HttpClient,
    model: String,
    policy: Option<Policy>,
    last_call_at: Arc<AtomicU64>,
}

/// Create a new [`Provider`] wired to OpenAI with Bearer auth.
pub fn new_adapter(cfg: &Config) -> AppResult<OpenAiAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::bearer(&cfg.api_key));

    let client = HttpClient::new(http_cfg)?;

    Ok(OpenAiAdapter {
        client,
        model: cfg.model.clone(),
        policy: None,
        last_call_at: Arc::new(AtomicU64::new(0)),
    })
}

impl OpenAiAdapter {
    /// Inject a resilience policy. Network calls are wrapped via `Policy::execute`.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    fn record_call(&self) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.last_call_at.store(now_ms, Ordering::Relaxed);
    }

    async fn complete_inner(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let body = OpenAiDialect::build_body(&req)?;

        let request = Request::post(OpenAiDialect::endpoint())
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(OpenAiDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read OpenAI response: {e}"),
            )
        })?;

        OpenAiDialect::parse_response(&text)
    }
}

#[async_trait]
impl rskit_provider::Provider for OpenAiAdapter {
    fn name(&self) -> &'static str {
        "openai"
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for OpenAiAdapter {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for OpenAiAdapter {
    async fn complete(&self, mut req: CompletionRequest) -> AppResult<CompletionResponse> {
        if req.model.is_empty() {
            req.model.clone_from(&self.model);
        }

        let span = tracing::info_span!("llm.complete");
        span.set_attribute(semconv::SYSTEM, SYSTEM);
        span.set_attribute(semconv::OPERATION_NAME, semconv::Operation::Chat.as_str());
        span.set_attribute(semconv::REQUEST_MODEL, req.model.clone());
        if let Some(max) = req.max_tokens {
            span.set_attribute(semconv::REQUEST_MAX_TOKENS, i64::from(max));
        }
        if let Some(temp) = req.temperature {
            span.set_attribute(semconv::REQUEST_TEMPERATURE, f64::from(temp));
        }

        let policy = self.policy.clone();

        async move {
            let response = if let Some(policy) = policy {
                let req = req.clone();
                policy
                    .execute(|| {
                        let req = req.clone();
                        async move { self.complete_inner(req).await }
                    })
                    .await?
            } else {
                self.complete_inner(req).await?
            };
            self.record_call();
            let current = tracing::Span::current();
            current.set_attribute(
                semconv::USAGE_INPUT_TOKENS,
                i64::try_from(response.usage.input_tokens).unwrap_or(i64::MAX),
            );
            current.set_attribute(
                semconv::USAGE_OUTPUT_TOKENS,
                i64::try_from(response.usage.output_tokens).unwrap_or(i64::MAX),
            );
            current.set_attribute(semconv::RESPONSE_MODEL, response.model.clone());
            if let Some(reason) = response.stop_reason.as_ref() {
                current.set_attribute(semconv::RESPONSE_FINISH_REASON, format!("{reason:?}"));
            }
            Ok::<_, AppError>(response)
        }
        .instrument(span)
        .await
    }
}

#[async_trait]
impl Component for OpenAiAdapter {
    fn name(&self) -> &str {
        "rskit-llm-openai.openai"
    }

    async fn start(&self) -> AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_adapter_constructs_successfully() {
        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn Provider> = Box::new(adapter);
    }

    #[test]
    fn adapter_implements_component() {
        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-openai.openai");
        assert!(component.health().is_healthy());
    }
}

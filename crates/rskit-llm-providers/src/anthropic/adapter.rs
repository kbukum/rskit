//! Adapter factory: bridges Anthropic [`Config`] → [`Provider`] via rskit-httpclient.

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
use super::dialect::AnthropicDialect;

const SYSTEM: &str = "anthropic";

/// A [`Provider`] backed by the Anthropic Messages API.
pub struct AnthropicAdapter {
    client: HttpClient,
    model: String,
    api_version: String,
    policy: Option<Policy>,
    last_call_at: Arc<AtomicU64>,
}

/// Create a new [`Provider`] wired to Anthropic with `x-api-key` + `anthropic-version` headers.
pub fn new_adapter(cfg: &Config) -> AppResult<AnthropicAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::api_key("x-api-key", &cfg.api_key))
        .with_header("anthropic-version", &cfg.api_version);

    let client = HttpClient::new(http_cfg)?;

    Ok(AnthropicAdapter {
        client,
        model: cfg.model.clone(),
        api_version: cfg.api_version.clone(),
        policy: None,
        last_call_at: Arc::new(AtomicU64::new(0)),
    })
}

impl AnthropicAdapter {
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
        let body = AnthropicDialect::build_body(&req)?;

        let request = Request::post(AnthropicDialect::endpoint())
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(AnthropicDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read Anthropic response: {e}"),
            )
        })?;

        AnthropicDialect::parse_response(&text)
    }
}

#[async_trait]
impl rskit_provider::Provider for AnthropicAdapter {
    fn name(&self) -> &'static str {
        "anthropic"
    }
}

#[async_trait]
impl Provider for AnthropicAdapter {
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
impl Component for AnthropicAdapter {
    fn name(&self) -> &str {
        "rskit-llm-providers.anthropic"
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
            api_key: "sk-ant-test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "sk-ant-test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn Provider> = Box::new(adapter);
    }

    #[test]
    fn adapter_implements_component() {
        let cfg = Config {
            api_key: "sk-ant-test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-providers.anthropic");
        assert!(component.health().is_healthy());
    }
}

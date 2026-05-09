//! Adapter factory: bridges Gemini [`Config`] → [`Provider`] via rskit-httpclient.
//!
//! Gemini authenticates via the `x-goog-api-key` HTTP header (never via
//! query string).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};
use rskit_resilience::Policy;
use tracing::Instrument;

use super::config::Config;
use super::dialect::GeminiDialect;

const SYSTEM: &str = "gemini";

/// A [`Provider`] backed by the Google Gemini API.
pub struct GeminiAdapter {
    client: HttpClient,
    model: String,
    api_key: String,
    policy: Option<Policy>,
    last_call_at: Arc<AtomicU64>,
}

/// Create a new [`Provider`] wired to Gemini with API key via the
/// `x-goog-api-key` request header.
pub fn new_adapter(cfg: &Config) -> AppResult<GeminiAdapter> {
    let http_cfg = HttpClientConfig::new().with_base_url(&cfg.base_url);

    let client = HttpClient::new(http_cfg)?;

    Ok(GeminiAdapter {
        client,
        model: cfg.model.clone(),
        api_key: cfg.api_key.clone(),
        policy: None,
        last_call_at: Arc::new(AtomicU64::new(0)),
    })
}

impl GeminiAdapter {
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
        let model = req.model.clone();
        let body = GeminiDialect::build_body(&req)?;
        let endpoint = GeminiDialect::endpoint(&model);

        let request = Request::post(endpoint)
            .header("x-goog-api-key", &self.api_key)
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(GeminiDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read Gemini response: {e}"),
            )
        })?;

        GeminiDialect::parse_response(&text, &model)
    }
}

#[async_trait]
impl rskit_provider::Provider for GeminiAdapter {
    fn name(&self) -> &'static str {
        "gemini"
    }
}

#[async_trait]
impl Provider for GeminiAdapter {
    async fn complete(&self, mut req: CompletionRequest) -> AppResult<CompletionResponse> {
        if req.model.is_empty() {
            req.model.clone_from(&self.model);
        }

        let span = tracing::info_span!("llm.complete");
        span.record(semconv::SYSTEM, SYSTEM);
        span.record(semconv::OPERATION_NAME, semconv::Operation::Chat.as_str());
        span.record(semconv::REQUEST_MODEL, req.model.as_str());
        if let Some(max) = req.max_tokens {
            span.record(semconv::REQUEST_MAX_TOKENS, max as i64);
        }
        if let Some(temp) = req.temperature {
            span.record(semconv::REQUEST_TEMPERATURE, temp as f64);
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
            current.record(semconv::USAGE_INPUT_TOKENS, response.usage.input_tokens);
            current.record(semconv::USAGE_OUTPUT_TOKENS, response.usage.output_tokens);
            current.record(semconv::RESPONSE_MODEL, response.model.as_str());
            if let Some(reason) = response.stop_reason {
                let reason_str = format!("{reason:?}");
                current.record(semconv::RESPONSE_FINISH_REASON, reason_str.as_str());
            }
            Ok::<_, AppError>(response)
        }
        .instrument(span)
        .await
    }
}

#[async_trait]
impl Component for GeminiAdapter {
    fn name(&self) -> &str {
        "rskit-llm-providers.gemini"
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
            api_key: "AIza-test".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "AIza-test".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn Provider> = Box::new(adapter);
    }

    #[test]
    fn adapter_implements_component() {
        let cfg = Config {
            api_key: "AIza-test".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-providers.gemini");
        assert!(component.health().is_healthy());
    }
}

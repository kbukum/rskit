//! Adapter factory: bridges Gemini [`Config`] → [`Provider`] via rskit-httpclient.
//!
//! Gemini authenticates via the `x-goog-api-key` HTTP header (never via
//! query string).

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::PROVIDER_ID;
use super::config::Config;
use super::dialect::GeminiDialect;
use rskit_llm_common::{ChatRunner, send_text};

const API_KEY_HEADER: &str = "x-goog-api-key";

/// A [`Provider`] backed by the Google Gemini API.
struct GeminiAdapter {
    client: HttpClient,
    runner: ChatRunner,
}

/// Create a new [`Provider`] wired to Gemini with API key via the
/// `x-goog-api-key` request header.
fn new_adapter(cfg: &Config) -> AppResult<GeminiAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::api_key_secret(API_KEY_HEADER, cfg.api_key.clone()));

    let client = HttpClient::new(http_cfg)?;

    Ok(GeminiAdapter {
        client,
        runner: ChatRunner::new(PROVIDER_ID, &cfg.model),
    })
}

/// Register the configured `Gemini` provider in an LLM registry.
pub fn register(registry: &mut rskit_llm::Registry, config: Config) -> AppResult<()> {
    registry.register(
        PROVIDER_ID,
        std::sync::Arc::new(move || {
            Ok(std::sync::Arc::new(new_adapter(&config)?)
                as std::sync::Arc<dyn rskit_llm::Provider>)
        }),
    )
}

impl GeminiAdapter {
    async fn complete_inner(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let model = req.model.clone();
        let body = GeminiDialect::build_body(&req)?;
        let endpoint = GeminiDialect::endpoint(&model);

        let request = Request::post(endpoint).json_body(&body).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
        })?;

        let text = send_text(
            &self.client,
            request,
            PROVIDER_ID,
            GeminiDialect::parse_error,
        )
        .await?;

        GeminiDialect::parse_response(&text, &model)
    }
}

#[async_trait]
impl rskit_provider::Provider for GeminiAdapter {
    fn name(&self) -> &'static str {
        PROVIDER_ID
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for GeminiAdapter {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for GeminiAdapter {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        self.runner
            .complete(req, |req| self.complete_inner(req))
            .await
    }
}

#[async_trait]
impl Component for GeminiAdapter {
    fn name(&self) -> &'static str {
        "rskit-llm-gemini.gemini"
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
    use rskit_util::SecretString;

    #[test]
    fn new_adapter_constructs_successfully() {
        let cfg = Config {
            api_key: SecretString::new("AIza-test"),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: SecretString::new("AIza-test"),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn Provider> = Box::new(adapter);
    }

    #[test]
    fn adapter_implements_component() {
        let cfg = Config {
            api_key: SecretString::new("AIza-test"),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-gemini.gemini");
        assert!(component.health().is_healthy());
    }
}

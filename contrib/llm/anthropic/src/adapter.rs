//! Adapter factory: bridges Anthropic [`Config`] → [`Provider`] via rskit-httpclient.

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::PROVIDER_ID;
use super::config::Config;
use super::dialect::AnthropicDialect;
use rskit_llm_common::{ChatRunner, send_text};

const API_KEY_HEADER: &str = "x-api-key";
const API_VERSION_HEADER: &str = "anthropic-version";

/// A [`Provider`] backed by the Anthropic Messages API.
struct AnthropicAdapter {
    client: HttpClient,
    api_version: String,
    runner: ChatRunner,
}

/// Create a new [`Provider`] wired to Anthropic with `x-api-key` + `anthropic-version` headers.
fn new_adapter(cfg: &Config) -> AppResult<AnthropicAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::api_key_secret(API_KEY_HEADER, cfg.api_key.clone()))
        .with_header(API_VERSION_HEADER, &cfg.api_version);

    let client = HttpClient::new(http_cfg)?;

    Ok(AnthropicAdapter {
        client,
        api_version: cfg.api_version.clone(),
        runner: ChatRunner::new(PROVIDER_ID, &cfg.model),
    })
}

/// Register the configured `Anthropic` provider in an LLM registry.
pub fn register(registry: &mut rskit_llm::Registry, config: Config) -> AppResult<()> {
    registry.register(
        PROVIDER_ID,
        std::sync::Arc::new(move || {
            Ok(std::sync::Arc::new(new_adapter(&config)?)
                as std::sync::Arc<dyn rskit_llm::Provider>)
        }),
    )
}

impl AnthropicAdapter {
    async fn complete_inner(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let body = AnthropicDialect::build_body(&req)?;

        let request = Request::post(AnthropicDialect::endpoint())
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let text = send_text(
            &self.client,
            request,
            PROVIDER_ID,
            AnthropicDialect::parse_error,
        )
        .await?;

        AnthropicDialect::parse_response(&text)
    }
}

#[async_trait]
impl rskit_provider::Provider for AnthropicAdapter {
    fn name(&self) -> &'static str {
        PROVIDER_ID
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for AnthropicAdapter {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for AnthropicAdapter {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        self.runner
            .complete(req, |req| self.complete_inner(req))
            .await
    }
}

#[async_trait]
impl Component for AnthropicAdapter {
    fn name(&self) -> &'static str {
        "rskit-llm-anthropic.anthropic"
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
            api_key: rskit_util::SecretString::new("sk-ant-test"),
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
            api_key: rskit_util::SecretString::new("sk-ant-test"),
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
            api_key: rskit_util::SecretString::new("sk-ant-test"),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-anthropic.anthropic");
        assert!(component.health().is_healthy());
    }
}

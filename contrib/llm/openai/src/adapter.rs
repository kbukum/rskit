//! Adapter factory: bridges `OpenAI` [`Config`] → [`Provider`] via `rskit-httpclient`.

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::PROVIDER_ID;
use super::config::Config;
use rskit_llm_common::{ChatRunner, OpenAiDialect, send_text};

/// A [`Provider`] backed by the `OpenAI` chat-completions API.
struct OpenAiAdapter {
    client: HttpClient,
    runner: ChatRunner,
}

/// Create a new [`Provider`] wired to `OpenAI` with Bearer auth.
fn new_adapter(cfg: &Config) -> AppResult<OpenAiAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::bearer_secret(cfg.api_key.clone()));

    let client = HttpClient::new(http_cfg)?;

    Ok(OpenAiAdapter {
        client,
        runner: ChatRunner::new(PROVIDER_ID, &cfg.model),
    })
}

/// Register the configured `OpenAI` provider in an LLM registry.
pub fn register(registry: &mut rskit_llm::Registry, config: Config) -> AppResult<()> {
    registry.register(
        PROVIDER_ID,
        std::sync::Arc::new(move || {
            Ok(std::sync::Arc::new(new_adapter(&config)?)
                as std::sync::Arc<dyn rskit_llm::Provider>)
        }),
    )
}

impl OpenAiAdapter {
    async fn complete_inner(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let body = OpenAiDialect::build_body(&req)?;

        let request = Request::post(OpenAiDialect::endpoint())
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let text = send_text(
            &self.client,
            request,
            PROVIDER_ID,
            OpenAiDialect::parse_error,
        )
        .await?;

        OpenAiDialect::parse_response(&text)
    }
}

#[async_trait]
impl rskit_provider::Provider for OpenAiAdapter {
    fn name(&self) -> &'static str {
        PROVIDER_ID
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
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        self.runner
            .complete(req, |req| self.complete_inner(req))
            .await
    }
}

#[async_trait]
impl Component for OpenAiAdapter {
    fn name(&self) -> &'static str {
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
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn Provider> = Box::new(adapter);
    }

    #[test]
    fn adapter_implements_component() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-openai.openai");
        assert!(component.health().is_healthy());
    }
}

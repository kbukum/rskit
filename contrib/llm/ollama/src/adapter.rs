//! Adapter factory: bridges Ollama [`Config`] → [`Provider`] via the `OpenAI` dialect.
//!
//! Ollama exposes an `OpenAI`-compatible chat-completions API. This adapter
//! delegates to [`OpenAiDialect`] for wire-format conversion — no second
//! dialect implementation is needed.

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::CompletionRequest;

use super::config::Config;
use rskit_llm_common::{ChatRunner, OpenAiDialect, send_text};

const SYSTEM: &str = "ollama";

/// A [`Provider`] backed by a local or remote Ollama instance.
///
/// Ollama mirrors the `OpenAI` `/v1/chat/completions` API, so this adapter
/// reuses [`OpenAiDialect`] for all wire-format conversion.
struct OllamaAdapter {
    client: HttpClient,
    runner: ChatRunner,
}

/// Create a new [`Provider`] wired to Ollama.
fn new_adapter(cfg: &Config) -> AppResult<OllamaAdapter> {
    let mut http_cfg = HttpClientConfig::new().with_base_url(&cfg.base_url);

    if let Some(key) = &cfg.api_key {
        http_cfg = http_cfg.with_auth(Auth::bearer(key));
    }

    let client = HttpClient::new(http_cfg)?;

    Ok(OllamaAdapter {
        client,
        runner: ChatRunner::new(SYSTEM, &cfg.model),
    })
}

/// Register the configured `Ollama` provider in an LLM registry.
pub fn register(registry: &mut rskit_llm::Registry, config: Config) -> AppResult<()> {
    registry.register(
        "ollama",
        std::sync::Arc::new(move || {
            Ok(std::sync::Arc::new(new_adapter(&config)?)
                as std::sync::Arc<dyn rskit_llm::Provider>)
        }),
    )
}

impl OllamaAdapter {
    async fn complete_inner(
        &self,
        req: CompletionRequest,
    ) -> AppResult<rskit_llm::types::CompletionResponse> {
        let body = OpenAiDialect::build_body(&req)?;

        let request = Request::post(OpenAiDialect::endpoint())
            .json_body(&body)
            .map_err(|e| AppError::internal(e).context("build Ollama request"))?;

        let text = send_text(&self.client, request, SYSTEM, OpenAiDialect::parse_error).await?;

        OpenAiDialect::parse_response(&text)
    }
}

#[async_trait]
impl rskit_provider::Provider for OllamaAdapter {
    fn name(&self) -> &'static str {
        SYSTEM
    }
}

#[async_trait]
impl
    rskit_provider::RequestResponse<
        rskit_llm::types::CompletionRequest,
        rskit_llm::types::CompletionResponse,
    > for OllamaAdapter
{
    async fn execute(
        &self,
        input: rskit_llm::types::CompletionRequest,
    ) -> AppResult<rskit_llm::types::CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for OllamaAdapter {
    async fn complete(
        &self,
        req: CompletionRequest,
    ) -> AppResult<rskit_llm::types::CompletionResponse> {
        self.runner
            .complete(req, |req| self.complete_inner(req))
            .await
    }
}

#[async_trait]
impl Component for OllamaAdapter {
    fn name(&self) -> &'static str {
        "rskit-llm-ollama.ollama"
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

//! Adapter factory: bridges Ollama [`Config`] → [`Provider`] via the `OpenAI` dialect.
//!
//! Ollama exposes an `OpenAI`-compatible chat-completions API.
//! This adapter delegates to [`OpenAiDialect`] for wire-format conversion —
//! no second dialect implementation is needed.

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::CompletionRequest;

use super::config::Config;
use rskit_llm_common::{ChatRunner, OpenAiDialect, send_text};

pub(crate) const PROVIDER_ID: &str = "ollama";

/// A [`Provider`] backed by a local or remote Ollama instance.
///
/// Ollama mirrors the `OpenAI` `/v1/chat/completions` API,
/// so this adapter reuses [`OpenAiDialect`] for all wire-format conversion.
struct OllamaAdapter {
    client: HttpClient,
    runner: ChatRunner,
}

/// Create a new [`Provider`] wired to Ollama.
fn new_adapter(cfg: &Config) -> AppResult<OllamaAdapter> {
    let mut http_cfg = HttpClientConfig::new().with_base_url(&cfg.base_url);

    if let Some(key) = &cfg.api_key {
        http_cfg = http_cfg.with_auth(Auth::bearer_secret(key.clone()));
    }

    let http_cfg = cfg.transport.apply_to(http_cfg);

    let client = HttpClient::new(http_cfg)?;

    Ok(OllamaAdapter {
        client,
        runner: ChatRunner::new(PROVIDER_ID, &cfg.model),
    })
}

/// Register the configured `Ollama` provider in an LLM registry.
pub fn register(registry: &mut rskit_llm::Registry, config: Config) -> AppResult<()> {
    registry.register(
        PROVIDER_ID,
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
impl rskit_provider::Provider for OllamaAdapter {
    fn name(&self) -> &'static str {
        PROVIDER_ID
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

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;
    use rskit_llm_common::HttpTransportConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn config(base_url: String) -> Config {
        Config {
            base_url,
            model: "llama3.2".into(),
            api_key: None,
            transport: HttpTransportConfig::default(),
        }
    }

    #[test]
    fn new_adapter_constructs_without_api_key() {
        let adapter = new_adapter(&config("http://localhost:11434".into())).unwrap();
        assert_eq!(rskit_provider::Provider::name(&adapter), PROVIDER_ID);
    }

    #[test]
    fn new_adapter_constructs_with_api_key() {
        let cfg = Config {
            api_key: Some(rskit_util::SecretString::new("ollama-token")),
            ..config("http://localhost:11434".into())
        };
        let adapter = new_adapter(&cfg).unwrap();
        let component: &dyn Component = &adapter;
        assert_eq!(component.name(), "rskit-llm-ollama.ollama");
        assert!(component.health().is_healthy());
    }

    #[test]
    fn register_adds_ollama_factory() {
        let mut registry = rskit_llm::Registry::new();
        register(&mut registry, config("http://localhost:11434".into())).unwrap();
        assert_eq!(registry.kinds(), vec![PROVIDER_ID]);
        assert!(registry.build(PROVIDER_ID).is_ok());
    }

    #[tokio::test]
    async fn complete_posts_openai_compatible_request() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("POST /chat/completions HTTP/1.1"));
            assert!(request.contains(r#""model":"llama3.2""#));
            assert!(request.contains(r#""content":"hello""#));

            let body = r#"{"model":"llama3.2","choices":[{"message":{"content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let adapter = new_adapter(&config(format!("http://{addr}"))).unwrap();
        let response = adapter
            .complete(types::CompletionRequest {
                model: String::new(),
                messages: vec![types::user("hello")],
                max_tokens: None,
                temperature: None,
                stream: false,
                tools: None,
                tool_choice: None,
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(response.model, "llama3.2");
        assert_eq!(response.text(), "hi");
        assert_eq!(response.usage.input_tokens, 3);
        assert_eq!(response.usage.output_tokens, 2);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn lifecycle_methods_are_noops() {
        let adapter = new_adapter(&config("http://localhost:11434".into())).unwrap();
        adapter.start().await.unwrap();
        adapter.stop().await.unwrap();
        assert!(adapter.health().is_healthy());
    }
}

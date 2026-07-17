//! Adapter factory: bridges `OpenAI` [`Config`] → [`Provider`] via `rskit-httpclient`.

use async_trait::async_trait;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::config::Config;
use rskit_llm_common::{ChatRunner, OpenAiDialect, send_text};

pub(crate) const PROVIDER_ID: &str = "openai";

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
        let request = openai_request(&req)?;

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

fn openai_request(req: &CompletionRequest) -> AppResult<Request> {
    let body = OpenAiDialect::build_body(req)?;
    Request::post(OpenAiDialect::endpoint())
        .json_body(&body)
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("failed to build request: {e}")))
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
    use rskit_llm::types;
    use rskit_provider::RequestResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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

    #[test]
    fn register_builds_provider_and_request_builder_accepts_completion() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
        };
        let mut registry = rskit_llm::Registry::new();
        register(&mut registry, cfg).unwrap();
        let provider = registry.build(PROVIDER_ID).unwrap();
        assert_eq!(provider.name(), PROVIDER_ID);

        let req = CompletionRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![types::user("hello")],
            max_tokens: Some(32),
            temperature: Some(0.2),
            stream: false,
            tools: None,
            tool_choice: None,
        };
        openai_request(&req).unwrap();
    }

    #[tokio::test]
    async fn component_lifecycle_methods_are_noops() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: Some(1536),
        };
        let adapter = new_adapter(&cfg).unwrap();

        assert_eq!(rskit_provider::Provider::name(&adapter), PROVIDER_ID);
        adapter.start().await.unwrap();
        adapter.stop().await.unwrap();
        assert!(adapter.health().is_healthy());
    }

    #[tokio::test]
    async fn complete_posts_to_local_openai_endpoint_and_parses_response() {
        let (base_url, server) = spawn_response_server(
            200,
            r#"{"model":"gpt-4o","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4}}"#,
        )
        .await;
        let adapter = new_adapter(&Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url,
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: None,
        })
        .unwrap();

        let response = adapter
            .complete(CompletionRequest {
                model: String::new(),
                messages: vec![types::user("hello")],
                max_tokens: Some(8),
                temperature: Some(0.1),
                stream: false,
                tools: None,
                tool_choice: None,
            })
            .await
            .unwrap();

        assert_eq!(response.model, "gpt-4o");
        assert_eq!(response.usage.input_tokens, 3);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn complete_maps_openai_error_response() {
        let (base_url, server) =
            spawn_response_server(429, r#"{"error":{"message":"rate limited"}}"#).await;
        let adapter = new_adapter(&Config {
            api_key: rskit_util::SecretString::new("sk-test"),
            base_url,
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: None,
        })
        .unwrap();

        let err = adapter
            .execute(CompletionRequest {
                model: "gpt-4o".into(),
                messages: vec![types::user("hello")],
                max_tokens: None,
                temperature: None,
                stream: false,
                tools: None,
                tool_choice: None,
            })
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::RateLimited);
        server.await.unwrap();
    }

    async fn spawn_response_server(
        status: u16,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 4096];
            let _ = socket.read(&mut buffer).await;
            let reason = if status >= 400 { "Error" } else { "OK" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        (format!("http://{address}"), server)
    }
}

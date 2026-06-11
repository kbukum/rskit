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
        let request = anthropic_request(&req, &self.api_version)?;

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

fn anthropic_request(req: &CompletionRequest, api_version: &str) -> AppResult<Request> {
    let body = AnthropicDialect::build_body(req)?;
    Request::post(AnthropicDialect::endpoint())
        .header("anthropic-version", api_version)
        .header("content-type", "application/json")
        .json_body(&body)
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("failed to build request: {e}")))
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
    use rskit_llm::types;
    use rskit_provider::RequestResponse;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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

    #[test]
    fn register_builds_provider_and_request_builder_accepts_completion() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-ant-test"),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let mut registry = rskit_llm::Registry::new();
        register(&mut registry, cfg).unwrap();
        let provider = registry.build(PROVIDER_ID).unwrap();
        assert_eq!(provider.name(), PROVIDER_ID);

        let req = CompletionRequest {
            model: "claude-sonnet-4-20250514".to_owned(),
            messages: vec![types::user("hello")],
            max_tokens: Some(32),
            temperature: Some(0.2),
            stream: false,
            tools: None,
            tool_choice: None,
        };
        anthropic_request(&req, "2023-06-01").unwrap();
    }

    #[tokio::test]
    async fn component_lifecycle_methods_are_noops() {
        let cfg = Config {
            api_key: rskit_util::SecretString::new("sk-ant-test"),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();

        assert_eq!(rskit_provider::Provider::name(&adapter), PROVIDER_ID);
        adapter.start().await.unwrap();
        adapter.stop().await.unwrap();
        assert!(adapter.health().is_healthy());
    }

    #[tokio::test]
    async fn complete_posts_to_local_anthropic_endpoint_and_parses_response() {
        let (base_url, server) = spawn_response_server(
            200,
            r#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-sonnet","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":6}}"#,
        )
        .await;
        let adapter = new_adapter(&Config {
            api_key: rskit_util::SecretString::new("sk-ant-test"),
            base_url,
            model: "claude-sonnet".into(),
            api_version: "2023-06-01".into(),
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

        assert_eq!(response.model, "claude-sonnet");
        assert_eq!(response.usage.output_tokens, 6);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn complete_maps_anthropic_error_response() {
        let (base_url, server) =
            spawn_response_server(401, r#"{"error":{"message":"bad key"}}"#).await;
        let adapter = new_adapter(&Config {
            api_key: rskit_util::SecretString::new("sk-ant-test"),
            base_url,
            model: "claude-sonnet".into(),
            api_version: "2023-06-01".into(),
        })
        .unwrap();

        let err = adapter
            .execute(CompletionRequest {
                model: "claude-sonnet".into(),
                messages: vec![types::user("hello")],
                max_tokens: None,
                temperature: None,
                stream: false,
                tools: None,
                tool_choice: None,
            })
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::Unauthorized);
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

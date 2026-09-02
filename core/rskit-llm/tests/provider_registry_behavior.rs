#![allow(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::{
    AssistantMessage, CompletionRequest, CompletionResponse, FinishReason, LlmRequestResponse,
    LlmStream, Provider, Registry, ToolChoice, Usage, text_content, user,
};
use rskit_provider::{Provider as _, RequestResponse, Stream as _};

struct EchoProvider {
    text: &'static str,
}

#[async_trait]
impl rskit_provider::Provider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for EchoProvider {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for EchoProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AppError> {
        Ok(CompletionResponse {
            message: AssistantMessage {
                content: text_content(self.text),
                tool_calls: vec![],
                usage: None,
            },
            model: request.model,
            usage: Usage {
                input_tokens: 2,
                output_tokens: 3,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(FinishReason::Stop),
        })
    }
}

fn request(stream: bool) -> CompletionRequest {
    CompletionRequest {
        model: "echo-model".into(),
        messages: vec![user("hello world")],
        max_tokens: Some(16),
        temperature: Some(0.1),
        stream,
        tools: None,
        tool_choice: Some(ToolChoice::auto()),
        ..Default::default()
    }
}

#[test]
fn registry_normalizes_kinds_orders_keys_and_reports_errors() {
    let mut registry = Registry::new();
    registry
        .register(
            " zed ",
            Arc::new(|| Ok(Arc::new(EchoProvider { text: "z" }) as Arc<dyn Provider>)),
        )
        .unwrap();
    registry
        .register(
            "alpha",
            Arc::new(|| Ok(Arc::new(EchoProvider { text: "a" }) as Arc<dyn Provider>)),
        )
        .unwrap();
    assert_eq!(registry.kinds(), vec!["alpha", "zed"]);
    assert!(registry.build("zed").is_ok());
    assert_eq!(
        registry
            .register("alpha", Arc::new(|| unreachable!()))
            .unwrap_err()
            .code(),
        ErrorCode::AlreadyExists
    );
    match registry.build("missing") {
        Ok(_) => panic!("missing kind should fail"),
        Err(error) => assert_eq!(error.code(), ErrorCode::NotFound),
    }
    match Registry::new().build(" ") {
        Ok(_) => panic!("empty kind should fail"),
        Err(error) => assert_eq!(error.code(), ErrorCode::InvalidInput),
    }
    assert!(rskit_llm::default_registry().kinds().is_empty());
}

#[test]
fn tool_choice_constructors_are_stable_contracts() {
    assert_eq!(ToolChoice::auto().mode, "auto");
    assert_eq!(ToolChoice::none().mode, "none");
    assert_eq!(ToolChoice::required().mode, "required");
    let specific = ToolChoice::specific("lookup");
    assert_eq!(specific.mode, "specific");
    assert_eq!(specific.function.as_deref(), Some("lookup"));
}

#[tokio::test]
async fn provider_default_stream_and_adapters_delegate_to_complete() {
    let provider = Arc::new(EchoProvider { text: "hi" });
    let response = provider.complete(request(false)).await.unwrap();
    assert_eq!(response.text(), "hi");
    assert!(!response.has_tool_calls());
    assert!(provider.count_tokens(&request(false).messages) > 0);

    let mut raw = provider.stream(request(true)).await.unwrap();
    let mut event_types = Vec::new();
    while let Some(event) = raw.next().await {
        event_types.push(event.event_type());
    }
    assert_eq!(
        event_types,
        vec!["message.start", "text.delta", "usage.delta", "message.stop"]
    );

    let rr = LlmRequestResponse(Arc::clone(&provider));
    assert_eq!(rr.name(), "echo");
    assert_eq!(
        rr.execute(request(false)).await.unwrap().model,
        "echo-model"
    );

    let stream_adapter = LlmStream(provider);
    let mut stream = stream_adapter.execute(request(true)).await.unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap().event_type(),
        "message.start"
    );
}

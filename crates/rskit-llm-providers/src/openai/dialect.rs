//! OpenAI chat-completions dialect.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, Message, StopReason,
    Usage,
};
use serde::{Deserialize, Serialize};

use crate::common;

/// Converts an rskit [`CompletionRequest`] into the OpenAI wire format and
/// parses the response back into an rskit [`CompletionResponse`].
pub struct OpenAiDialect;

// --- Wire types ---

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct WireMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<Choice>,
    usage: WireUsage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct WireUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// --- Conversion helpers ---

fn to_wire(msg: &Message) -> WireMessage {
    match msg {
        Message::System(s) => WireMessage {
            role: "system".to_string(),
            content: s.content.clone(),
        },
        Message::User(u) => WireMessage {
            role: "user".to_string(),
            content: rskit_llm::types::text_of(&u.content),
        },
        Message::Assistant(a) => WireMessage {
            role: "assistant".to_string(),
            content: rskit_llm::types::text_of(&a.content),
        },
        Message::ToolResult(tr) => WireMessage {
            role: "tool".to_string(),
            content: tr.content.clone(),
        },
    }
}

impl OpenAiDialect {
    /// Build the JSON body for a chat completion request.
    pub fn build_body(req: &CompletionRequest) -> AppResult<serde_json::Value> {
        let wire = ChatRequest {
            model: req.model.clone(),
            messages: req.messages.iter().map(to_wire).collect(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };
        serde_json::to_value(&wire).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize OpenAI request: {e}"),
            )
        })
    }

    /// Parse a successful chat-completion response body.
    pub fn parse_response(body: &str) -> AppResult<CompletionResponse> {
        let resp: ChatResponse = serde_json::from_str(body).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse OpenAI response: {e}"),
            )
        })?;

        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: vec![ContentBlock::Text { text: content }],
                tool_calls: vec![],
                usage: None,
            },
            model: resp.model,
            usage: Usage {
                input_tokens: resp.usage.prompt_tokens,
                output_tokens: resp.usage.completion_tokens,
            },
            stop_reason: Some(StopReason::EndTurn),
        })
    }

    /// Parse an error response body into an [`AppError`].
    pub fn parse_error(status: u16, body: &str) -> AppError {
        common::parse_openai_error(status, body).into()
    }

    /// Returns the chat completions endpoint path.
    pub fn endpoint() -> &'static str {
        "/chat/completions"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[test]
    fn build_body_produces_valid_json() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![types::user("hello")],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: false,
            tools: None,
            tool_choice: None,
        };
        let body = OpenAiDialect::build_body(&req).unwrap();
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn parse_response_extracts_content() {
        let body = r#"{
            "id": "chatcmpl-1",
            "model": "gpt-4o",
            "choices": [{"message": {"content": "Hi there!"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;
        let resp = OpenAiDialect::parse_response(body).unwrap();
        assert_eq!(resp.text(), "Hi there!");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
    }

    #[test]
    fn endpoint_is_correct() {
        assert_eq!(OpenAiDialect::endpoint(), "/chat/completions");
    }
}

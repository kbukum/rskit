//! Anthropic Messages API dialect.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, Message, StopReason,
    Usage,
};
use serde::{Deserialize, Serialize};

use crate::common;

/// Converts rskit types to/from the Anthropic Messages API wire format.
pub struct AnthropicDialect;

// --- Wire types ---

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<WireMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
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
    content: Vec<ContentItem>,
    usage: WireUsage,
}

#[derive(Deserialize)]
struct ContentItem {
    text: Option<String>,
}

#[derive(Deserialize)]
struct WireUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// --- Conversion helpers ---

fn to_wire(msg: &Message) -> Option<WireMessage> {
    match msg {
        Message::User(u) => Some(WireMessage {
            role: "user".to_string(),
            content: rskit_llm::types::text_of(&u.content),
        }),
        Message::Assistant(a) => Some(WireMessage {
            role: "assistant".to_string(),
            content: rskit_llm::types::text_of(&a.content),
        }),
        Message::ToolResult(tr) => Some(WireMessage {
            role: "user".to_string(),
            content: tr.content.clone(),
        }),
        Message::System(_) => None, // handled via top-level `system` field
    }
}

impl AnthropicDialect {
    /// Build the JSON body for the Anthropic Messages API.
    pub fn build_body(req: &CompletionRequest) -> AppResult<serde_json::Value> {
        let system_msg = req.messages.iter().find_map(|m| match m {
            Message::System(s) => Some(s.content.clone()),
            _ => None,
        });

        let wire = ChatRequest {
            model: req.model.clone(),
            messages: req.messages.iter().filter_map(to_wire).collect(),
            max_tokens: req.max_tokens.unwrap_or(1024),
            temperature: req.temperature,
            system: system_msg,
        };
        serde_json::to_value(&wire).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize Anthropic request: {e}"),
            )
        })
    }

    /// Parse a successful Messages API response body.
    pub fn parse_response(body: &str) -> AppResult<CompletionResponse> {
        let resp: ChatResponse = serde_json::from_str(body).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse Anthropic response: {e}"),
            )
        })?;

        let content_text = resp
            .content
            .into_iter()
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: vec![ContentBlock::Text { text: content_text }],
                tool_calls: vec![],
                usage: None,
            },
            model: resp.model,
            usage: Usage {
                input_tokens: resp.usage.input_tokens,
                output_tokens: resp.usage.output_tokens,
            },
            stop_reason: Some(StopReason::EndTurn),
        })
    }

    /// Parse an error response body into an [`AppError`].
    pub fn parse_error(status: u16, body: &str) -> AppError {
        common::parse_anthropic_error(status, body).into()
    }

    /// Returns the messages endpoint path.
    pub fn endpoint() -> &'static str {
        "/v1/messages"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[test]
    fn build_body_extracts_system_message() {
        let req = CompletionRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![types::system("You are helpful."), types::user("hello")],
            max_tokens: Some(200),
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };
        let body = AnthropicDialect::build_body(&req).unwrap();
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn parse_response_extracts_content() {
        let body = r#"{
            "id": "msg-1",
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "Hello!"}],
            "usage": {"input_tokens": 8, "output_tokens": 3}
        }"#;
        let resp = AnthropicDialect::parse_response(body).unwrap();
        assert_eq!(resp.text(), "Hello!");
        assert_eq!(resp.usage.input_tokens, 8);
    }

    #[test]
    fn endpoint_is_correct() {
        assert_eq!(AnthropicDialect::endpoint(), "/v1/messages");
    }
}

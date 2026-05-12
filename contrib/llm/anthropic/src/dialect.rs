//! Anthropic Messages API dialect.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentPart, FinishReason, Message,
    ToolUseBlock, Usage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use rskit_llm_common as common;
use rskit_llm_common::{StreamChunk, StreamToolCall};

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
    model: String,
    content: Vec<ContentItem>,
    usage: WireUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ContentItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Map<String, Value>>,
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
        Message::Tool(tr) => Some(WireMessage {
            role: "user".to_string(),
            content: tr.content.clone(),
        }),
        Message::System(_) => None,
    }
}

fn parse_stop_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("error") => FinishReason::Error,
        Some("cancelled") => FinishReason::Cancelled,
        _ => FinishReason::Stop,
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

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for (index, item) in resp.content.into_iter().enumerate() {
            match item.kind.as_str() {
                "text" => content.push_str(item.text.as_deref().unwrap_or("")),
                "tool_use" => tool_calls.push(ToolUseBlock {
                    id: item.id.unwrap_or_else(|| format!("tool_call_{index}")),
                    name: item.name.unwrap_or_default(),
                    input: item.input.unwrap_or_default(),
                }),
                _ => {}
            }
        }

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: vec![ContentPart::Text { text: content }],
                tool_calls,
                usage: None,
            },
            model: resp.model,
            usage: Usage {
                input_tokens: u64::from(resp.usage.input_tokens),
                output_tokens: u64::from(resp.usage.output_tokens),
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(parse_stop_reason(resp.stop_reason.as_deref())),
        })
    }

    /// Parse an error response body into an [`AppError`].
    pub fn parse_error(status: u16, body: &str) -> AppError {
        common::parse_anthropic_error(status, body).into()
    }

    /// Parse a single SSE data payload from the streaming Messages API.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn parse_stream_chunk(data: &[u8]) -> AppResult<StreamChunk> {
        let s = std::str::from_utf8(data).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("invalid UTF-8 in Anthropic stream chunk: {e}"),
            )
        })?;

        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse Anthropic stream chunk: {e}"),
            )
        })?;

        let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "content_block_delta" => {
                let delta = v.get("delta").unwrap_or(&serde_json::Value::Null);
                let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        let text = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        Ok(StreamChunk {
                            content: text.to_string(),
                            ..Default::default()
                        })
                    }
                    "input_json_delta" => {
                        let partial = delta
                            .get("partial_json")
                            .and_then(|p| p.as_str())
                            .unwrap_or("");
                        let index = v
                            .get("index")
                            .and_then(Value::as_u64)
                            .map_or(0, |value| usize::try_from(value).unwrap_or(usize::MAX));
                        Ok(StreamChunk {
                            tool_calls: vec![StreamToolCall {
                                index,
                                id: String::new(),
                                name: String::new(),
                                input_delta: partial.to_string(),
                            }],
                            ..Default::default()
                        })
                    }
                    _ => Ok(StreamChunk::default()),
                }
            }
            "content_block_start" => {
                let block = v.get("content_block").unwrap_or(&serde_json::Value::Null);
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

                if block_type == "tool_use" {
                    let id = block
                        .get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let index = v
                        .get("index")
                        .and_then(Value::as_u64)
                        .map_or(0, |value| usize::try_from(value).unwrap_or(usize::MAX));
                    Ok(StreamChunk {
                        tool_calls: vec![StreamToolCall {
                            index,
                            id,
                            name,
                            input_delta: String::new(),
                        }],
                        ..Default::default()
                    })
                } else {
                    Ok(StreamChunk::default())
                }
            }
            "message_stop" => Ok(StreamChunk {
                done: true,
                ..Default::default()
            }),
            _ => Ok(StreamChunk::default()),
        }
    }

    /// Returns the messages endpoint path.
    pub const fn endpoint() -> &'static str {
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
            "usage": {"input_tokens": 8, "output_tokens": 3},
            "stop_reason": "end_turn"
        }"#;
        let resp = AnthropicDialect::parse_response(body).unwrap();
        assert_eq!(resp.text(), "Hello!");
        assert_eq!(resp.usage.input_tokens, 8);
        assert_eq!(resp.stop_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn endpoint_is_correct() {
        assert_eq!(AnthropicDialect::endpoint(), "/v1/messages");
    }

    #[test]
    fn stream_chunk_text_delta() {
        let data = br#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let chunk = AnthropicDialect::parse_stream_chunk(data).unwrap();
        assert_eq!(chunk.content, "Hello");
        assert!(chunk.tool_calls.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_tool_use_start() {
        let data = br#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather"}}"#;
        let chunk = AnthropicDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].id, "toolu_1");
        assert_eq!(chunk.tool_calls[0].name, "get_weather");
        assert!(chunk.tool_calls[0].input_delta.is_empty());
    }

    #[test]
    fn stream_chunk_tool_input_delta() {
        let data = br#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"loc\":"}}"#;
        let chunk = AnthropicDialect::parse_stream_chunk(data).unwrap();
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].input_delta, r#"{"loc":"#);
    }

    #[test]
    fn stream_chunk_message_stop() {
        let data = br#"{"type":"message_stop"}"#;
        let chunk = AnthropicDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.done);
        assert!(chunk.content.is_empty());
    }

    #[test]
    fn stream_chunk_unknown_event_type() {
        let data = br#"{"type":"ping"}"#;
        let chunk = AnthropicDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert!(chunk.tool_calls.is_empty());
        assert!(!chunk.done);
    }
}

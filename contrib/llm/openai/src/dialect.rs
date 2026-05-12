//! OpenAI chat-completions dialect.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentPart, FinishReason, Message,
    ToolUseBlock, Usage,
};
use serde::{Deserialize, Serialize};

use rskit_llm_common as common;
use rskit_llm_common::{StreamChunk, StreamToolCall};

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
    model: String,
    choices: Vec<Choice>,
    usage: WireUsage,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Deserialize)]
struct ResponseToolCall {
    #[serde(default)]
    id: String,
    function: ResponseFunction,
}

#[derive(Deserialize)]
struct ResponseFunction {
    name: String,
    arguments: String,
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
        Message::Tool(tr) => WireMessage {
            role: "tool".to_string(),
            content: tr.content.clone(),
        },
    }
}

fn parse_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolUse,
        Some("content_filter") => FinishReason::ContentFilter,
        Some("error") => FinishReason::Error,
        Some("cancelled") => FinishReason::Cancelled,
        _ => FinishReason::Stop,
    }
}

fn parse_tool_call(tool_call: ResponseToolCall, index: usize) -> AppResult<ToolUseBlock> {
    Ok(ToolUseBlock {
        id: if tool_call.id.is_empty() {
            format!("tool_call_{index}")
        } else {
            tool_call.id
        },
        name: tool_call.function.name,
        input: common::parse_input_json(&tool_call.function.arguments)?,
    })
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

        let choice = resp.choices.into_iter().next();
        let (content, tool_calls, stop_reason) = match choice {
            Some(choice) => {
                let tool_calls = choice
                    .message
                    .tool_calls
                    .into_iter()
                    .enumerate()
                    .map(|(index, tool_call)| parse_tool_call(tool_call, index))
                    .collect::<AppResult<Vec<_>>>()?;
                (
                    choice.message.content.unwrap_or_default(),
                    tool_calls,
                    parse_finish_reason(choice.finish_reason.as_deref()),
                )
            }
            None => (String::new(), Vec::new(), FinishReason::Stop),
        };

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: vec![ContentPart::Text { text: content }],
                tool_calls,
                usage: None,
            },
            model: resp.model,
            usage: Usage {
                input_tokens: u64::from(resp.usage.prompt_tokens),
                output_tokens: u64::from(resp.usage.completion_tokens),
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(stop_reason),
        })
    }

    /// Parse an error response body into an [`AppError`].
    pub fn parse_error(status: u16, body: &str) -> AppError {
        common::parse_openai_error(status, body).into()
    }

    /// Parse a single SSE data payload from the streaming chat-completions API.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn parse_stream_chunk(data: &[u8]) -> AppResult<StreamChunk> {
        let s = std::str::from_utf8(data).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("invalid UTF-8 in OpenAI stream chunk: {e}"),
            )
        })?;

        if s.trim() == "[DONE]" {
            return Ok(StreamChunk {
                done: true,
                ..Default::default()
            });
        }

        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse OpenAI stream chunk: {e}"),
            )
        })?;

        let choice = match v.get("choices").and_then(|c| c.get(0)) {
            Some(c) => c,
            None => return Ok(StreamChunk::default()),
        };

        let delta = choice.get("delta").unwrap_or(&serde_json::Value::Null);

        let content = delta
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let mut tool_calls = Vec::new();
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let index = tc
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .map_or(0, |value| value as usize);
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let func = tc.get("function").unwrap_or(&serde_json::Value::Null);
                let name = func
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_delta = func
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tool_calls.push(StreamToolCall {
                    index,
                    id,
                    name,
                    input_delta,
                });
            }
        }

        let done = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .is_some_and(|reason| !reason.is_empty());

        Ok(StreamChunk {
            content,
            tool_calls,
            done,
        })
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
            "choices": [{"message": {"content": "Hi there!"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;
        let resp = OpenAiDialect::parse_response(body).unwrap();
        assert_eq!(resp.text(), "Hi there!");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.stop_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn endpoint_is_correct() {
        assert_eq!(OpenAiDialect::endpoint(), "/chat/completions");
    }

    #[test]
    fn stream_chunk_content_delta() {
        let data = br#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert_eq!(chunk.content, "Hello");
        assert!(chunk.tool_calls.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_tool_call() {
        let data = br#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"loc\":"}}]},"finish_reason":null}]}"#;
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].id, "call_1");
        assert_eq!(chunk.tool_calls[0].name, "get_weather");
        assert_eq!(chunk.tool_calls[0].input_delta, r#"{"loc":"#);
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_done_signal() {
        let data = b"[DONE]";
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.done);
        assert!(chunk.content.is_empty());
    }

    #[test]
    fn stream_chunk_finish_reason_stop() {
        let data = br#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn stream_chunk_empty_choices() {
        let data = br#"{"choices":[]}"#;
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert!(!chunk.done);
    }
}

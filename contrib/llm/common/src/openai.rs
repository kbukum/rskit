//! `OpenAI` chat-completions dialect.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentPart, FinishReason, Message,
    ToolUseBlock, Usage,
};
use serde::{Deserialize, Serialize};

use crate as common;
use crate::{StreamChunk, StreamToolCall};

/// Converts an rskit [`CompletionRequest`] into the `OpenAI` wire format
/// and parses the response back into an rskit [`CompletionResponse`].
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
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
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
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if let Some(system) = &req.system_prompt {
            messages.push(WireMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }
        messages.extend(req.messages.iter().map(to_wire));

        let wire = ChatRequest {
            model: req.model.clone(),
            messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            stop: req.stop_sequences.clone(),
            stream: false,
        };
        let mut body = serde_json::to_value(&wire).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize OpenAI request: {e}"),
            )
        })?;
        common::merge_extra(&mut body, &req.extra);
        Ok(body)
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
    pub fn parse_stream_chunk(data: &[u8]) -> AppResult<StreamChunk> {
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

        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            return Ok(StreamChunk::default());
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
                    .and_then(serde_json::Value::as_u64)
                    .map_or(0, |value| usize::try_from(value).unwrap_or(usize::MAX));
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
    pub const fn endpoint() -> &'static str {
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
            ..Default::default()
        };
        let body = OpenAiDialect::build_body(&req).unwrap();
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn build_body_maps_all_message_roles() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                types::system("be brief"),
                types::assistant("previous"),
                types::tool_result_msg("call_1", "result", false),
            ],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
            ..Default::default()
        };

        let body = OpenAiDialect::build_body(&req).unwrap();

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][2]["role"], "tool");
    }

    #[test]
    fn build_body_wires_sampling_system_and_extra_fields() {
        let mut req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![types::user("hello")],
            max_tokens: Some(64),
            temperature: Some(0.5),
            stream: false,
            tools: None,
            tool_choice: None,
            system_prompt: Some("be terse".to_string()),
            top_p: Some(0.9),
            stop_sequences: vec!["STOP".to_string()],
            ..Default::default()
        };
        req.extra.insert("seed".to_string(), serde_json::json!(7));

        let body = OpenAiDialect::build_body(&req).unwrap();

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be terse");
        assert_eq!(body["messages"][1]["role"], "user");
        let top_p = body["top_p"].as_f64().unwrap();
        assert!((top_p - 0.9).abs() < 1e-6);
        assert_eq!(body["stop"][0], "STOP");
        assert_eq!(body["seed"], 7);
    }

    #[test]
    fn build_body_extra_does_not_override_typed_fields() {
        let mut req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![types::user("hi")],
            ..Default::default()
        };
        req.extra
            .insert("model".to_string(), serde_json::json!("evil"));

        let body = OpenAiDialect::build_body(&req).unwrap();

        assert_eq!(body["model"], "gpt-4o");
    }

    #[test]
    fn build_body_omits_optional_sampling_fields_when_unset() {
        let req = CompletionRequest {
            model: "gpt-4o".to_string(),
            messages: vec![types::user("hi")],
            ..Default::default()
        };
        let body = OpenAiDialect::build_body(&req).unwrap();
        assert!(body.get("top_p").is_none());
        assert!(body.get("stop").is_none());
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
    fn parse_response_handles_empty_choices_tool_calls_and_finish_reasons() {
        let empty = OpenAiDialect::parse_response(
            r#"{"model":"gpt-4o","choices":[],"usage":{"prompt_tokens":0,"completion_tokens":0}}"#,
        )
        .unwrap();
        assert!(empty.text().is_empty());
        assert_eq!(empty.stop_reason, Some(FinishReason::Stop));

        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{"id":"","function":{"name":"lookup","arguments":"{\"q\":\"rust\"}"}}]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        }"#;
        let resp = OpenAiDialect::parse_response(body).unwrap();
        assert_eq!(resp.stop_reason, Some(FinishReason::ToolUse));
        assert_eq!(resp.message.tool_calls[0].id, "tool_call_0");
        assert_eq!(resp.message.tool_calls[0].input["q"], "rust");

        for (wire, expected) in [
            ("length", FinishReason::Length),
            ("content_filter", FinishReason::ContentFilter),
            ("error", FinishReason::Error),
            ("cancelled", FinishReason::Cancelled),
        ] {
            let body = format!(
                r#"{{"model":"gpt-4o","choices":[{{"message":{{"content":"x"}},"finish_reason":"{wire}"}}],"usage":{{"prompt_tokens":1,"completion_tokens":1}}}}"#
            );
            let resp = OpenAiDialect::parse_response(&body).unwrap();
            assert_eq!(resp.stop_reason, Some(expected));
        }
    }

    #[test]
    fn parse_response_rejects_invalid_json_and_tool_arguments() {
        assert_eq!(
            OpenAiDialect::parse_response("{").unwrap_err().code(),
            ErrorCode::ExternalService
        );
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{"message": {"tool_calls": [{"function":{"name":"lookup","arguments":"not-json"}}]}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        }"#;
        assert_eq!(
            OpenAiDialect::parse_response(body).unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
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

    #[test]
    fn stream_chunk_rejects_invalid_utf8_or_json_and_defaults_tool_fields() {
        assert_eq!(
            OpenAiDialect::parse_stream_chunk(&[0xff])
                .unwrap_err()
                .code(),
            ErrorCode::InvalidFormat
        );
        assert_eq!(
            OpenAiDialect::parse_stream_chunk(b"{").unwrap_err().code(),
            ErrorCode::ExternalService
        );

        let data =
            br#"{"choices":[{"delta":{"tool_calls":[{"index":18446744073709551615,"function":{}}]},"finish_reason":"tool_calls"}]}"#;
        let chunk = OpenAiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.tool_calls[0].index, usize::MAX);
        assert!(chunk.tool_calls[0].id.is_empty());
        assert!(chunk.tool_calls[0].name.is_empty());
        assert!(chunk.tool_calls[0].input_delta.is_empty());
    }
}

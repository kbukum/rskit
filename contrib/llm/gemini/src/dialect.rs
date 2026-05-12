//! Gemini Generative Language API dialect.
//!
//! Gemini uses a structurally different API from `OpenAI` / `Anthropic`:
//! - Endpoint: `POST /v1beta/models/{model}:generateContent`
//! - Content uses `parts` (`text`, `functionCall`, `functionResponse`)
//! - System prompt → `systemInstruction`
//! - Config → `generationConfig` (`temperature`, `maxOutputTokens`, `topP`, `stopSequences`)
//! - Response: `candidates[].content.parts[]`, `usageMetadata`
//! - Stop reasons: `STOP`, `MAX_TOKENS`, `SAFETY`, `TOOL_USE`

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentPart, FinishReason, Message,
    ToolUseBlock, Usage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use rskit_llm_common as common;
use rskit_llm_common::{StreamChunk, StreamToolCall};

/// Converts rskit types to/from the Gemini `generateContent` wire format.
pub struct GeminiDialect;

// --- Wire types (request/response) ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<WireContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<WireContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct WireContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<WirePart>,
}

#[derive(Serialize, Deserialize)]
struct WirePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<WireFunctionCall>,
}

#[derive(Clone, Serialize, Deserialize)]
struct WireFunctionCall {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<WireContent>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

// --- Conversion helpers ---

fn to_wire_content(msg: &Message) -> Option<WireContent> {
    match msg {
        Message::User(u) => Some(WireContent {
            role: Some("user".to_string()),
            parts: vec![WirePart {
                text: Some(rskit_llm::types::text_of(&u.content)),
                function_call: None,
            }],
        }),
        Message::Assistant(a) => Some(WireContent {
            role: Some("model".to_string()),
            parts: vec![WirePart {
                text: Some(rskit_llm::types::text_of(&a.content)),
                function_call: None,
            }],
        }),
        Message::Tool(tr) => Some(WireContent {
            role: Some("user".to_string()),
            parts: vec![WirePart {
                text: Some(tr.content.clone()),
                function_call: None,
            }],
        }),
        Message::System(_) => None,
    }
}

fn parse_stop_reason(reason: &str) -> FinishReason {
    if reason == "MAX_TOKENS" {
        FinishReason::Length
    } else if reason == "SAFETY" {
        FinishReason::ContentFilter
    } else if reason == "TOOL_USE" {
        FinishReason::ToolUse
    } else if reason == "CANCELLED" {
        FinishReason::Cancelled
    } else {
        FinishReason::Stop
    }
}

fn parse_function_call(function_call: WireFunctionCall, index: usize) -> AppResult<ToolUseBlock> {
    Ok(ToolUseBlock {
        id: format!("tool_call_{index}"),
        name: function_call.name,
        input: common::value_to_input_map(
            function_call
                .args
                .unwrap_or_else(|| Value::Object(Map::new())),
        )?,
    })
}

impl GeminiDialect {
    /// Build the JSON body for a `generateContent` request.
    pub fn build_body(req: &CompletionRequest) -> AppResult<serde_json::Value> {
        let system_instruction = req.messages.iter().find_map(|m| match m {
            Message::System(s) => Some(WireContent {
                role: None,
                parts: vec![WirePart {
                    text: Some(s.content.clone()),
                    function_call: None,
                }],
            }),
            _ => None,
        });

        let generation_config = if req.temperature.is_some() || req.max_tokens.is_some() {
            Some(GenerationConfig {
                temperature: req.temperature,
                max_output_tokens: req.max_tokens,
            })
        } else {
            None
        };

        let wire = GenerateContentRequest {
            contents: req.messages.iter().filter_map(to_wire_content).collect(),
            system_instruction,
            generation_config,
        };

        serde_json::to_value(&wire).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize Gemini request: {e}"),
            )
        })
    }

    /// Parse a successful `generateContent` response body.
    pub fn parse_response(body: &str, model: &str) -> AppResult<CompletionResponse> {
        let resp: GenerateContentResponse = serde_json::from_str(body).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse Gemini response: {e}"),
            )
        })?;

        let candidate = resp.candidates.as_ref().and_then(|c| c.first());

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        if let Some(parts) = candidate
            .and_then(|c| c.content.as_ref())
            .map(|content| &content.parts)
        {
            for (index, part) in parts.iter().enumerate() {
                if let Some(text) = part.text.as_deref() {
                    content.push_str(text);
                }
                if let Some(function_call) = part.function_call.clone() {
                    tool_calls.push(parse_function_call(function_call, index)?);
                }
            }
        }

        let stop_reason = candidate
            .and_then(|c| c.finish_reason.as_deref())
            .map_or(FinishReason::Stop, parse_stop_reason);

        let usage = resp.usage_metadata.as_ref().map_or(
            Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            |u| Usage {
                input_tokens: u64::from(u.prompt_token_count.unwrap_or(0)),
                output_tokens: u64::from(u.candidates_token_count.unwrap_or(0)),
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
        );

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: vec![ContentPart::Text { text: content }],
                tool_calls,
                usage: None,
            },
            model: model.to_string(),
            usage,
            stop_reason: Some(stop_reason),
        })
    }

    /// Parse an error response body into an [`AppError`].
    pub fn parse_error(status: u16, body: &str) -> AppError {
        common::parse_gemini_error(status, body).into()
    }

    /// Parse a single SSE data payload from the streaming `generateContent` API.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn parse_stream_chunk(data: &[u8]) -> AppResult<StreamChunk> {
        let s = std::str::from_utf8(data).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("invalid UTF-8 in Gemini stream chunk: {e}"),
            )
        })?;

        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse Gemini stream chunk: {e}"),
            )
        })?;

        let candidate = v.get("candidates").and_then(|c| c.get(0));
        let parts = candidate
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array());

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(parts) = parts {
            for (index, part) in parts.iter().enumerate() {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    content.push_str(text);
                }
                if let Some(function_call) = part.get("functionCall") {
                    let name = function_call
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input_delta = function_call
                        .get("args")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Map::new()))
                        .to_string();
                    tool_calls.push(StreamToolCall {
                        index,
                        id: String::new(),
                        name,
                        input_delta,
                    });
                }
            }
        }

        let done = candidate
            .and_then(|c| c.get("finishReason"))
            .and_then(|r| r.as_str())
            .is_some_and(|reason| !reason.is_empty() && reason != "NONE");

        Ok(StreamChunk {
            content,
            tool_calls,
            done,
        })
    }

    /// Build the endpoint path for a given model.
    pub fn endpoint(model: &str) -> String {
        format!("/v1beta/models/{model}:generateContent")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[test]
    fn build_body_with_system_instruction() {
        let req = CompletionRequest {
            model: "gemini-2.5-flash".to_string(),
            messages: vec![types::system("You are helpful."), types::user("hello")],
            max_tokens: Some(100),
            temperature: Some(0.8),
            stream: false,
            tools: None,
            tool_choice: None,
        };
        let body = GeminiDialect::build_body(&req).unwrap();
        assert!(
            body["systemInstruction"]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .contains("helpful")
        );
        assert_eq!(body["contents"].as_array().unwrap().len(), 1);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 100);
        let temp = body["generationConfig"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.8).abs() < 1e-6);
    }

    #[test]
    fn build_body_model_role_for_assistant() {
        let req = CompletionRequest {
            model: "gemini-2.5-flash".to_string(),
            messages: vec![
                types::user("hi"),
                types::assistant("hello"),
                types::user("how?"),
            ],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };
        let body = GeminiDialect::build_body(&req).unwrap();
        assert_eq!(body["contents"][1]["role"], "model");
    }

    #[test]
    fn parse_response_extracts_content() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello!"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 2
            }
        }"#;
        let resp = GeminiDialect::parse_response(body, "gemini-2.5-flash").unwrap();
        assert_eq!(resp.text(), "Hello!");
        assert_eq!(resp.model, "gemini-2.5-flash");
        assert_eq!(resp.usage.input_tokens, 5);
        assert_eq!(resp.usage.output_tokens, 2);
        assert_eq!(resp.stop_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn parse_response_max_tokens_stop_reason() {
        let body = r#"{
            "candidates": [{
                "content": {"parts": [{"text": "partial"}], "role": "model"},
                "finishReason": "MAX_TOKENS"
            }],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 10}
        }"#;
        let resp = GeminiDialect::parse_response(body, "gemini-2.5-flash").unwrap();
        assert_eq!(resp.stop_reason, Some(FinishReason::Length));
    }

    #[test]
    fn parse_response_safety_stop_reason() {
        let body = r#"{
            "candidates": [{
                "content": {"parts": [{"text": ""}], "role": "model"},
                "finishReason": "SAFETY"
            }]
        }"#;
        let resp = GeminiDialect::parse_response(body, "gemini-2.5-flash").unwrap();
        assert_eq!(resp.stop_reason, Some(FinishReason::ContentFilter));
    }

    #[test]
    fn endpoint_includes_model() {
        assert_eq!(
            GeminiDialect::endpoint("gemini-2.5-flash"),
            "/v1beta/models/gemini-2.5-flash:generateContent"
        );
    }

    #[test]
    fn stream_chunk_text_content() {
        let data = br#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}]}"#;
        let chunk = GeminiDialect::parse_stream_chunk(data).unwrap();
        assert_eq!(chunk.content, "Hello");
        assert!(chunk.tool_calls.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_function_call() {
        let data = br#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC"}}}],"role":"model"}}]}"#;
        let chunk = GeminiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].name, "get_weather");
        let args: serde_json::Value =
            serde_json::from_str(&chunk.tool_calls[0].input_delta).unwrap();
        assert_eq!(args["location"], "NYC");
    }

    #[test]
    fn stream_chunk_finish_reason_stop() {
        let data = br#"{"candidates":[{"content":{"parts":[{"text":"done"}],"role":"model"},"finishReason":"STOP"}]}"#;
        let chunk = GeminiDialect::parse_stream_chunk(data).unwrap();
        assert_eq!(chunk.content, "done");
        assert!(chunk.done);
    }

    #[test]
    fn stream_chunk_no_candidates() {
        let data = br#"{"candidates":[]}"#;
        let chunk = GeminiDialect::parse_stream_chunk(data).unwrap();
        assert!(chunk.content.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_finish_reason_none_is_not_done() {
        let data = br#"{"candidates":[{"content":{"parts":[{"text":"hi"}],"role":"model"},"finishReason":"NONE"}]}"#;
        let chunk = GeminiDialect::parse_stream_chunk(data).unwrap();
        assert!(!chunk.done);
    }
}

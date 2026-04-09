//! Streaming event types for LLM provider responses.
//!
//! [`StreamEvent`] represents the incremental chunks emitted by a streaming
//! completion. Consumers can pattern-match on the variant to update UI, accumulate
//! tool-call arguments, track token usage, etc.

use serde::{Deserialize, Serialize};

use crate::types::{CompletionResponse, Usage};

/// An incremental event emitted during a streaming completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A chunk of content text.
    ContentDelta { text: String },

    /// A chunk of a tool-call invocation.
    ToolCallDelta {
        tool_use_id: String,
        name: Option<String>,
        arguments_chunk: String,
    },

    /// A chunk of model thinking / chain-of-thought.
    ThinkingDelta { text: String },

    /// Updated token usage statistics.
    UsageUpdate { usage: Usage },

    /// Signals the start of a new message.
    MessageStart { model: String, role: String },

    /// The final assembled response — sent once when the stream ends.
    MessageComplete { response: CompletionResponse },

    /// An error that occurred during streaming.
    StreamError { error: String, code: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantMessage, Usage};

    #[test]
    fn test_content_delta_serde() {
        let event = StreamEvent::ContentDelta {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_delta");
        assert_eq!(json["text"], "Hello");

        let deser: StreamEvent = serde_json::from_value(json).unwrap();
        match deser {
            StreamEvent::ContentDelta { text } => assert_eq!(text, "Hello"),
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn test_tool_call_delta_serde() {
        let event = StreamEvent::ToolCallDelta {
            tool_use_id: "tc_1".to_string(),
            name: Some("get_weather".to_string()),
            arguments_chunk: r#"{"loc"#.to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "tool_call_delta");
        assert_eq!(json["tool_use_id"], "tc_1");
        assert_eq!(json["name"], "get_weather");
    }

    #[test]
    fn test_tool_call_delta_no_name() {
        let event = StreamEvent::ToolCallDelta {
            tool_use_id: "tc_1".to_string(),
            name: None,
            arguments_chunk: r#"ation":"#.to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("name").unwrap().is_null());
    }

    #[test]
    fn test_thinking_delta_serde() {
        let event = StreamEvent::ThinkingDelta {
            text: "Let me think...".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "thinking_delta");
    }

    #[test]
    fn test_usage_update_serde() {
        let event = StreamEvent::UsageUpdate {
            usage: Usage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "usage_update");
        assert_eq!(json["usage"]["input_tokens"], 100);
        assert_eq!(json["usage"]["output_tokens"], 50);
    }

    #[test]
    fn test_message_start_serde() {
        let event = StreamEvent::MessageStart {
            model: "gpt-4".to_string(),
            role: "assistant".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_start");
        assert_eq!(json["model"], "gpt-4");
    }

    #[test]
    fn test_message_complete_serde() {
        let response = CompletionResponse {
            message: AssistantMessage {
                content: vec![],
                tool_calls: vec![],
                usage: None,
            },
            model: "gpt-4".to_string(),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 20,
            },
            stop_reason: None,
        };
        let event = StreamEvent::MessageComplete { response };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_complete");
        assert_eq!(json["response"]["model"], "gpt-4");
    }

    #[test]
    fn test_stream_error_serde() {
        let event = StreamEvent::StreamError {
            error: "rate limited".to_string(),
            code: Some("429".to_string()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "stream_error");
        assert_eq!(json["error"], "rate limited");
        assert_eq!(json["code"], "429");
    }

    #[test]
    fn test_stream_error_no_code() {
        let event = StreamEvent::StreamError {
            error: "unknown".to_string(),
            code: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("code").unwrap().is_null());
    }
}

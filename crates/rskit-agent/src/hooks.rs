//! Domain-specific hook event types for the agentic pipeline.
//!
//! Each struct implements [`rskit_hook::Event`] so it can be emitted through
//! a [`rskit_hook::HookRegistry`].

use std::any::Any;

use rskit_hook::{Event, EventType};
use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse};
use rskit_tool::ToolResult;

// ── Event type constants ────────────────────────────────────────────────────

/// Event type for [`PreToolCall`].
pub fn pre_tool_call_type() -> EventType {
    EventType::new("pre_tool_call")
}

/// Event type for [`PostToolCall`].
pub fn post_tool_call_type() -> EventType {
    EventType::new("post_tool_call")
}

/// Event type for [`PreLLMCall`].
pub fn pre_llm_call_type() -> EventType {
    EventType::new("pre_llm_call")
}

/// Event type for [`PostLLMCall`].
pub fn post_llm_call_type() -> EventType {
    EventType::new("post_llm_call")
}

/// Event type for [`OnError`].
pub fn on_error_type() -> EventType {
    EventType::new("on_error")
}

/// Event type for [`TurnStart`].
pub fn turn_start_type() -> EventType {
    EventType::new("turn_start")
}

/// Event type for [`TurnEnd`].
pub fn turn_end_type() -> EventType {
    EventType::new("turn_end")
}

// ── Event structs ───────────────────────────────────────────────────────────

/// Fired before a tool call is executed.
#[derive(Debug, Clone)]
pub struct PreToolCall {
    pub name: String,
    pub input: serde_json::Value,
}

impl Event for PreToolCall {
    fn event_type(&self) -> EventType {
        pre_tool_call_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired after a tool call completes.
#[derive(Debug, Clone)]
pub struct PostToolCall {
    pub name: String,
    pub input: serde_json::Value,
    pub result: Option<ToolResult>,
    pub error: Option<String>,
}

impl Event for PostToolCall {
    fn event_type(&self) -> EventType {
        post_tool_call_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired before an LLM completion request is sent.
#[derive(Debug, Clone)]
pub struct PreLLMCall {
    pub request: CompletionRequest,
}

impl Event for PreLLMCall {
    fn event_type(&self) -> EventType {
        pre_llm_call_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired after an LLM completion response is received.
#[derive(Debug, Clone)]
pub struct PostLLMCall {
    pub response: CompletionResponse,
    pub error: Option<String>,
}

impl Event for PostLLMCall {
    fn event_type(&self) -> EventType {
        post_llm_call_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired when an error occurs anywhere in the pipeline.
#[derive(Debug, Clone)]
pub struct OnError {
    pub error: String,
    pub source: String,
}

impl Event for OnError {
    fn event_type(&self) -> EventType {
        on_error_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired at the start of an agent turn.
#[derive(Debug, Clone)]
pub struct TurnStart {
    pub turn: u32,
}

impl Event for TurnStart {
    fn event_type(&self) -> EventType {
        turn_start_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fired at the end of an agent turn.
#[derive(Debug, Clone)]
pub struct TurnEnd {
    pub turn: u32,
    pub message: AssistantMessage,
}

impl Event for TurnEnd {
    fn event_type(&self) -> EventType {
        turn_end_type()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_hook::EventType;

    #[test]
    fn test_pre_tool_call_event() {
        let event = PreToolCall {
            name: "calculator".to_string(),
            input: serde_json::json!({"x": 1}),
        };
        assert_eq!(event.event_type(), EventType::new("pre_tool_call"));
        let any = event.as_any();
        let downcasted = any.downcast_ref::<PreToolCall>().unwrap();
        assert_eq!(downcasted.name, "calculator");
    }

    #[test]
    fn test_post_tool_call_event() {
        let event = PostToolCall {
            name: "calculator".to_string(),
            input: serde_json::json!({}),
            result: None,
            error: Some("timeout".to_string()),
        };
        assert_eq!(event.event_type(), EventType::new("post_tool_call"));
    }

    #[test]
    fn test_pre_llm_call_event() {
        let event = PreLLMCall {
            request: CompletionRequest {
                model: "test".to_string(),
                messages: vec![],
                max_tokens: None,
                temperature: None,
                stream: false,
                tools: None,
                tool_choice: None,
            },
        };
        assert_eq!(event.event_type(), EventType::new("pre_llm_call"));
    }

    #[test]
    fn test_on_error_event() {
        let event = OnError {
            error: "boom".to_string(),
            source: "agent".to_string(),
        };
        assert_eq!(event.event_type(), EventType::new("on_error"));
    }

    #[test]
    fn test_turn_start_event() {
        let event = TurnStart { turn: 0 };
        assert_eq!(event.event_type(), EventType::new("turn_start"));
        let any = event.as_any();
        assert_eq!(any.downcast_ref::<TurnStart>().unwrap().turn, 0);
    }

    #[test]
    fn test_turn_end_event() {
        use rskit_llm::types;
        let event = TurnEnd {
            turn: 3,
            message: AssistantMessage {
                content: types::text_content("done"),
                tool_calls: vec![],
                usage: None,
            },
        };
        assert_eq!(event.event_type(), EventType::new("turn_end"));
    }

    #[test]
    fn test_event_type_constants() {
        assert_eq!(pre_tool_call_type(), EventType::new("pre_tool_call"));
        assert_eq!(post_tool_call_type(), EventType::new("post_tool_call"));
        assert_eq!(pre_llm_call_type(), EventType::new("pre_llm_call"));
        assert_eq!(post_llm_call_type(), EventType::new("post_llm_call"));
        assert_eq!(on_error_type(), EventType::new("on_error"));
        assert_eq!(turn_start_type(), EventType::new("turn_start"));
        assert_eq!(turn_end_type(), EventType::new("turn_end"));
    }
}

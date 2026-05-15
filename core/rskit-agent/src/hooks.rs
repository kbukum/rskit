//! Domain-specific hook event types for the agentic pipeline.
//!
//! Each struct implements [`rskit_hook::Event`] so it can be emitted through
//! a [`rskit_hook::HookRegistry`].

use rskit_hook::{Event, EventType};
use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse};
use rskit_tool::ToolResult;

// ── Event type constants ────────────────────────────────────────────────────

/// Event type for canonical `on_tool_call` observations.
pub fn on_tool_call_type() -> EventType {
    EventType::new("on_tool_call")
}

/// Event type for [`PreToolCall`].
pub fn pre_tool_call_type() -> EventType {
    on_tool_call_type()
}

/// Event type for canonical `on_tool_result` observations.
pub fn on_tool_result_type() -> EventType {
    EventType::new("on_tool_result")
}

/// Event type for tool-result observations.
pub fn post_tool_call_type() -> EventType {
    on_tool_result_type()
}

/// Event type for canonical `on_llm_call` observations.
pub fn on_llm_call_type() -> EventType {
    EventType::new("on_llm_call")
}

/// Event type for [`PreLLMCall`].
pub fn pre_llm_call_type() -> EventType {
    on_llm_call_type()
}

/// Event type for canonical `on_llm_response` observations.
pub fn on_llm_response_type() -> EventType {
    EventType::new("on_llm_response")
}

/// Event type for LLM-response observations.
pub fn post_llm_call_type() -> EventType {
    on_llm_response_type()
}

/// Event type for [`OnError`].
pub fn on_error_type() -> EventType {
    EventType::new("on_error")
}

/// Event type for canonical [`TurnStart`].
pub fn on_turn_start_type() -> EventType {
    EventType::new("on_turn_start")
}

/// Event type for [`TurnStart`].
pub fn turn_start_type() -> EventType {
    on_turn_start_type()
}

/// Event type for canonical [`TurnEnd`].
pub fn on_turn_complete_type() -> EventType {
    EventType::new("on_turn_complete")
}

/// Event type for [`TurnEnd`].
pub fn turn_end_type() -> EventType {
    on_turn_complete_type()
}

/// Event type for canonical stream/event observations.
pub fn on_event_type() -> EventType {
    EventType::new("on_event")
}

/// Event type for canonical MCP call observations.
pub fn on_mcp_call_type() -> EventType {
    EventType::new("on_mcp_call")
}

/// Event type for canonical MCP result observations.
pub fn on_mcp_result_type() -> EventType {
    EventType::new("on_mcp_result")
}

// ── Event structs ───────────────────────────────────────────────────────────

/// Canonical observe-only event stream observation.
#[derive(Debug, Clone)]
pub struct OnEvent {
    /// Observed event payload.
    pub event: rskit_ai::StreamEventRef,
}

impl Event for OnEvent {
    fn event_type(&self) -> EventType {
        on_event_type()
    }
}

/// Canonical observe-only MCP call event.
#[derive(Debug, Clone)]
pub struct OnMCPCall {
    /// MCP method or operation name.
    pub method: String,
}

impl Event for OnMCPCall {
    fn event_type(&self) -> EventType {
        on_mcp_call_type()
    }
}

/// Canonical observe-only MCP result event.
#[derive(Debug, Clone)]
pub struct OnMCPResult {
    /// MCP method or operation name.
    pub method: String,
    /// Optional result payload.
    pub result: Option<serde_json::Value>,
    /// Optional error text.
    pub error: Option<String>,
}

impl Event for OnMCPResult {
    fn event_type(&self) -> EventType {
        on_mcp_result_type()
    }
}

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
        assert_eq!(event.event_type(), EventType::new("on_tool_call"));
        assert_eq!(event.name, "calculator");
    }

    #[test]
    fn test_post_tool_call_event() {
        let event = PostToolCall {
            name: "calculator".to_string(),
            input: serde_json::json!({}),
            result: None,
            error: Some("timeout".to_string()),
        };
        assert_eq!(event.event_type(), EventType::new("on_tool_result"));
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
        assert_eq!(event.event_type(), EventType::new("on_llm_call"));
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
        assert_eq!(event.event_type(), EventType::new("on_turn_start"));
        assert_eq!(event.turn, 0);
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
        assert_eq!(event.event_type(), EventType::new("on_turn_complete"));
    }

    #[test]
    fn test_event_type_constants() {
        assert_eq!(pre_tool_call_type(), EventType::new("on_tool_call"));
        assert_eq!(post_tool_call_type(), EventType::new("on_tool_result"));
        assert_eq!(pre_llm_call_type(), EventType::new("on_llm_call"));
        assert_eq!(post_llm_call_type(), EventType::new("on_llm_response"));
        assert_eq!(on_error_type(), EventType::new("on_error"));
        assert_eq!(turn_start_type(), EventType::new("on_turn_start"));
        assert_eq!(turn_end_type(), EventType::new("on_turn_complete"));
    }
}

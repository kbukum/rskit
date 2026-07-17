use rskit_hook::{Event, EventType};
use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse};
use rskit_tool::{ToolInput, ToolResult};

use super::{
    on_error_type, on_event_type, on_mcp_call_type, on_mcp_result_type, post_llm_call_type,
    post_tool_call_type, pre_llm_call_type, pre_tool_call_type, turn_end_type, turn_start_type,
};

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
    /// Tool name requested by the model.
    pub name: String,
    /// Tool input payload passed to the executor.
    pub input: ToolInput,
}

impl Event for PreToolCall {
    fn event_type(&self) -> EventType {
        pre_tool_call_type()
    }
}

/// Fired after a tool call completes.
#[derive(Debug, Clone)]
pub struct PostToolCall {
    /// Tool name requested by the model.
    pub name: String,
    /// Tool input payload passed to the executor.
    pub input: ToolInput,
    /// Successful tool result, when execution completed.
    pub result: Option<ToolResult>,
    /// Error text, when execution failed.
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
    /// Completion request that will be sent to the provider.
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
    /// Completion response returned by the provider.
    pub response: CompletionResponse,
    /// Error text, when the provider call failed.
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
    /// Error message reported by the failing operation.
    pub error: String,
    /// Pipeline stage or component that reported the error.
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
    /// Turn number being started.
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
    /// Turn number that completed.
    pub turn: u32,
    /// Assistant message produced during the turn.
    pub message: AssistantMessage,
}

impl Event for TurnEnd {
    fn event_type(&self) -> EventType {
        turn_end_type()
    }
}

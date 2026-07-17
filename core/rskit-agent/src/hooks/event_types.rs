use rskit_hook::EventType;

// ── Event type constants ────────────────────────────────────────────────────

/// Event type for canonical `on_tool_call` observations.
pub fn on_tool_call_type() -> EventType {
    EventType::new("on_tool_call")
}

/// Event type for [`PreToolCall`](super::PreToolCall).
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

/// Event type for [`PreLLMCall`](super::PreLLMCall).
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

/// Event type for [`OnError`](super::OnError).
pub fn on_error_type() -> EventType {
    EventType::new("on_error")
}

/// Event type for canonical [`TurnStart`](super::TurnStart).
pub fn on_turn_start_type() -> EventType {
    EventType::new("on_turn_start")
}

/// Event type for [`TurnStart`](super::TurnStart).
pub fn turn_start_type() -> EventType {
    on_turn_start_type()
}

/// Event type for canonical [`TurnEnd`](super::TurnEnd).
pub fn on_turn_complete_type() -> EventType {
    EventType::new("on_turn_complete")
}

/// Event type for [`TurnEnd`](super::TurnEnd).
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

//! Domain-specific hook event types for the agentic pipeline.
//!
//! Each struct implements [`rskit_hook::Event`]
//! so it can be emitted through a [`rskit_hook::HookRegistry`].

mod event_types;
mod events;

pub use event_types::{
    on_error_type, on_event_type, on_llm_call_type, on_llm_response_type, on_mcp_call_type,
    on_mcp_result_type, on_tool_call_type, on_tool_result_type, on_turn_complete_type,
    on_turn_start_type, post_llm_call_type, post_tool_call_type, pre_llm_call_type,
    pre_tool_call_type, turn_end_type, turn_start_type,
};
pub use events::{
    OnError, OnEvent, OnMCPCall, OnMCPResult, PostLLMCall, PostToolCall, PreLLMCall, PreToolCall,
    TurnEnd, TurnStart,
};

#[cfg(test)]
mod tests;

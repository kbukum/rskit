//! Agentic loop — Provider + Tools + Hooks in a turn-based execution engine.
//!
//! The [`Agent`] drives a multi-turn loop: it sends messages to an LLM
//! [`rskit_llm::Provider`], executes any requested tool calls, emits hook events at each
//! lifecycle point, and manages context size via a pluggable [`ContextStrategy`].

pub mod agent;
pub mod command;
pub mod hooks;
pub mod memory;
pub mod types;

pub use agent::{Agent, AgentConfig};
pub use command::{Command, CommandHandler, CommandRegistry, register_builtins};
pub use hooks::{
    OnError, OnEvent, OnMCPCall, OnMCPResult, PostLLMCall, PostToolCall, PreLLMCall, PreToolCall,
    TurnEnd, TurnStart, on_error_type, on_event_type, on_llm_call_type, on_llm_response_type,
    on_mcp_call_type, on_mcp_result_type, on_tool_call_type, on_tool_result_type,
    on_turn_complete_type, on_turn_start_type, post_llm_call_type, post_tool_call_type,
    pre_llm_call_type, pre_tool_call_type, turn_end_type, turn_start_type,
};
pub use memory::{InMemoryStore, Memory, SlidingWindowMemory};
pub use types::{
    AgentEvent, AgentResult, ContextStrategy, FailStrategy, StopReason, TruncateStrategy,
};

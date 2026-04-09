//! Agentic loop — Provider + Tools + Hooks in a turn-based execution engine.
//!
//! The [`Agent`] drives a multi-turn loop: it sends messages to an LLM
//! [`Provider`], executes any requested tool calls, emits hook events at each
//! lifecycle point, and manages context size via a pluggable [`ContextStrategy`].

pub mod agent;
pub mod types;

pub use agent::{Agent, AgentConfig};
pub use types::{
    AgentEvent, AgentResult, ContextStrategy, FailStrategy, StopReason, TruncateStrategy,
};

//! Tool definition, auto-wiring, registry and middleware for agentic systems.
//!
//! Provides a type-safe framework for defining tools that can be used in agentic systems,
//! LLM function calling, or MCP servers.

mod callable;
pub mod context;
mod definition;
pub mod envelope;
#[cfg(feature = "validation")]
mod from_fn;
pub mod hitl;
pub mod io;
pub mod registry;
pub mod result;

pub use callable::Callable;
pub use context::Context;
pub use definition::{Annotations, Definition, ExecutionHint};
pub use envelope::{
    DataClassification, Envelope, FilesystemMode, FilesystemRule, NetworkPolicy, NetworkRule,
    Safety, SensitiveMatcher, SensitivePredicate, SubprocessRule,
};
#[cfg(feature = "validation")]
pub use from_fn::{from_fn, from_fn_simple};
pub use hitl::{
    Decision, DenyHumanApproval, DenyOnSensitive, HumanApproval, SensitivityEvaluator, ToolCall,
    denied_error,
};
pub use io::{ToolInput, ToolMetadata, ToolOutput, ToolSchema};
pub use registry::{BatchOptions, Registry};
pub use result::{ToolResult, error_result, json_result, text_result};

#[cfg(test)]
mod tests;

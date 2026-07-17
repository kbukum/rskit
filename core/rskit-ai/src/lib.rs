//! Shared AI vocabulary for rskit AI/ML crates.
//!
//! This crate contains data shapes only: no I/O, providers, registries, or runtime logic.

#![warn(missing_docs)]

/// Budget vocabulary shared by GenAI orchestration layers.
pub mod budget;
/// Chat-completion-specific types.
pub mod chat;
/// Multimodal content and tool block vocabulary.
pub mod content;
/// Typed GenAI error sentinels.
pub mod error;
/// Provider and model identity vocabulary.
pub mod model;
/// Prompt templates, registries, and renderers.
pub mod prompt;
/// Canonical AI message roles.
pub mod role;
/// OpenTelemetry AI semantic convention keys.
pub mod semconv;
/// Streaming response event vocabulary.
pub mod stream;
/// Usage and cost vocabulary.
pub mod usage;
/// Vector math helpers shared across AI crates.
pub mod vector;

pub use budget::{Budget, BudgetExceededReason};
pub use content::{ContentPart, ToolResultBlock, ToolUseBlock, text_content, text_of};
pub use error::GenAiError;
pub use model::{Capabilities, Model, Provider};
pub use prompt::{
    Builder, PromptError, PromptIdentity, PromptTemplate, Registry, RenderContext, RenderToMessage,
    Template, ValidationFinding, ValidationFindingKind, VariableDecl, VariableType, render,
    validate,
};
pub use role::Role;
pub use stream::{
    ErrorEvent, FinishReason, MessageStart, MessageStop, ReasoningDelta, StreamEvent,
    StreamEventRef, TextDelta, ToolUseDelta, ToolUseStart, ToolUseStop, UsageDelta,
};
pub use usage::{Cost, Money, Usage};

#[cfg(test)]
mod tests;

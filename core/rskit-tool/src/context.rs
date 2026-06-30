//! Execution context for tool calls.

use tokio_util::sync::CancellationToken;

use crate::io::{ToolMetadata, ToolOutput};

/// Carries per-request metadata and cancellation through tool execution.
#[derive(Clone)]
pub struct Context {
    /// Unique request identifier.
    pub request_id: String,
    /// Tool-use identifier (links back to the LLM tool call).
    pub tool_use_id: String,
    /// Maximum size (in bytes) for the result content.
    pub max_result_size: usize,
    metadata: ToolMetadata,
    cancel_token: CancellationToken,
}

impl Context {
    /// Create a context with empty identifiers, no result-size limit, and a fresh cancellation token.
    pub fn new() -> Self {
        Self {
            request_id: String::new(),
            tool_use_id: String::new(),
            max_result_size: 0,
            metadata: ToolMetadata::new(),
            cancel_token: CancellationToken::new(),
        }
    }

    /// Create a context that observes the provided cancellation token.
    pub fn with_cancellation(token: CancellationToken) -> Self {
        Self {
            cancel_token: token,
            ..Self::new()
        }
    }

    /// Store metadata for the current tool call under `key`.
    pub fn set(&mut self, key: &str, value: ToolOutput) {
        self.metadata.insert(key.to_string(), value);
    }

    /// Return metadata previously stored under `key`, if present.
    pub fn get(&self, key: &str) -> Option<&ToolOutput> {
        self.metadata.get(key)
    }

    /// Return the cancellation token associated with this tool call.
    pub const fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// Return whether cancellation has been requested for this tool call.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("request_id", &self.request_id)
            .field("tool_use_id", &self.tool_use_id)
            .field("max_result_size", &self.max_result_size)
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

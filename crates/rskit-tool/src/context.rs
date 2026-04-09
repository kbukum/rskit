//! Execution context for tool calls.

use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Carries per-request metadata and cancellation through tool execution.
#[derive(Clone)]
pub struct Context {
    /// Unique request identifier.
    pub request_id: String,
    /// Tool-use identifier (links back to the LLM tool call).
    pub tool_use_id: String,
    /// Maximum size (in bytes) for the result content.
    pub max_result_size: usize,
    metadata: HashMap<String, serde_json::Value>,
    cancel_token: CancellationToken,
}

impl Context {
    pub fn new() -> Self {
        Self {
            request_id: String::new(),
            tool_use_id: String::new(),
            max_result_size: 0,
            metadata: HashMap::new(),
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn with_cancellation(token: CancellationToken) -> Self {
        Self {
            cancel_token: token,
            ..Self::new()
        }
    }

    pub fn set(&mut self, key: &str, value: serde_json::Value) {
        self.metadata.insert(key.to_string(), value);
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

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
            .finish()
    }
}

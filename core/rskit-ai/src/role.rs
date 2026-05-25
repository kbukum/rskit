//! Canonical AI message roles.

use serde::{Deserialize, Serialize};

/// Canonical AI message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    /// System/developer instruction.
    System,
    /// End-user message.
    User,
    /// Assistant/model message.
    Assistant,
    /// Tool result message.
    Tool,
}

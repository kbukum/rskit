use rskit_ai::StreamEventRef;
use rskit_llm::types::{AssistantMessage, Usage};
use rskit_tool::{ToolInput, ToolResult};

use super::AgentResult;

/// Events emitted during an agent run (for streaming / observability).
#[non_exhaustive]
pub enum AgentEvent {
    /// A new turn is starting.
    TurnStart {
        /// Turn number being started.
        turn: u32,
    },
    /// An LLM streaming event was received.
    LlmDelta {
        /// Streaming event received from the LLM provider.
        event: StreamEventRef,
    },
    /// A tool is about to be executed.
    ToolExecuting {
        /// Provider tool-use identifier.
        tool_use_id: String,
        /// Tool name.
        name: String,
        /// Tool input payload.
        input: ToolInput,
    },
    /// A tool call completed.
    ToolComplete {
        /// Provider tool-use identifier.
        tool_use_id: String,
        /// Tool name.
        name: String,
        /// Successful tool result, when available.
        result: Option<ToolResult>,
        /// Error text, when execution failed.
        error: Option<String>,
    },
    /// Context was compacted to fit within the token budget.
    Compacted {
        /// Previous approximate token count.
        old_tokens: usize,
        /// New approximate token count.
        new_tokens: usize,
    },
    /// A turn completed.
    TurnComplete {
        /// Completed turn number.
        turn: u32,
        /// Assistant message produced by the turn.
        message: AssistantMessage,
        /// Token usage for the turn.
        usage: Usage,
    },
    /// The agent run is complete.
    RunComplete {
        /// Final agent result.
        result: AgentResult,
    },
}

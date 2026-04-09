//! Agent result types, events, and context strategies.

use rskit_errors::AppError;
use rskit_llm::stream_events::StreamEvent;
use rskit_llm::types::{AssistantMessage, Message, Usage};
use rskit_tool::ToolResult;
use serde::{Deserialize, Serialize};

// ── StopReason ──────────────────────────────────────────────────────────────

/// Why the agent loop terminated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    /// The model finished its response without requesting any tool calls.
    EndTurn,
    /// Reached the configured maximum number of turns.
    MaxTurns,
    /// Exceeded the token budget.
    MaxBudget,
    /// Aborted by a hook handler.
    Aborted,
}

// ── AgentResult ─────────────────────────────────────────────────────────────

/// The final outcome of an agent run.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// All messages accumulated during the run (user + assistant + tool results).
    pub messages: Vec<Message>,
    /// The last assistant message before the loop ended.
    pub final_message: AssistantMessage,
    /// Aggregate token usage across all turns.
    pub total_usage: Usage,
    /// How many turns the agent executed.
    pub turn_count: u32,
    /// Why the agent stopped.
    pub stop_reason: StopReason,
}

// ── AgentEvent ──────────────────────────────────────────────────────────────

/// Events emitted during an agent run (for streaming / observability).
pub enum AgentEvent {
    /// A new turn is starting.
    TurnStart { turn: u32 },
    /// An LLM streaming event was received.
    LlmStreamEvent { event: StreamEvent },
    /// A tool is about to be executed.
    ToolExecuting {
        tool_use_id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool call completed.
    ToolComplete {
        tool_use_id: String,
        name: String,
        result: Option<ToolResult>,
        error: Option<String>,
    },
    /// Context was compacted to fit within the token budget.
    ContextCompacted {
        old_tokens: usize,
        new_tokens: usize,
    },
    /// A turn completed.
    TurnComplete {
        turn: u32,
        message: AssistantMessage,
        usage: Usage,
    },
    /// The agent run is complete.
    Complete { result: AgentResult },
}

// ── ContextStrategy ─────────────────────────────────────────────────────────

/// Strategy for compacting the message history when context is too large.
pub trait ContextStrategy: Send + Sync {
    /// Compact the message list so it fits within `max_tokens`.
    fn compact(&self, messages: Vec<Message>, max_tokens: usize) -> Result<Vec<Message>, AppError>;
}

/// Fail immediately if context exceeds the limit (no compaction).
pub struct FailStrategy;

impl ContextStrategy for FailStrategy {
    fn compact(
        &self,
        _messages: Vec<Message>,
        _max_tokens: usize,
    ) -> Result<Vec<Message>, AppError> {
        Err(AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "context exceeds token limit and no compaction strategy is configured",
        ))
    }
}

/// Keep only the system prompt (first message) and the last `keep_last` messages.
pub struct TruncateStrategy {
    /// Number of recent messages to preserve.
    pub keep_last: usize,
}

impl ContextStrategy for TruncateStrategy {
    fn compact(
        &self,
        messages: Vec<Message>,
        _max_tokens: usize,
    ) -> Result<Vec<Message>, AppError> {
        if messages.len() <= self.keep_last + 1 {
            return Ok(messages);
        }

        let mut result = Vec::with_capacity(self.keep_last + 1);
        // Keep system prompt if present
        if let Some(first) = messages.first() {
            if matches!(first, Message::System(_)) {
                result.push(first.clone());
            }
        }
        // Keep last N messages
        let start = messages.len().saturating_sub(self.keep_last);
        for msg in &messages[start..] {
            // Avoid duplicating system prompt
            if matches!(msg, Message::System(_)) && !result.is_empty() {
                continue;
            }
            result.push(msg.clone());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[test]
    fn test_stop_reason_serde() {
        let json = serde_json::to_string(&StopReason::EndTurn).unwrap();
        let deser: StopReason = serde_json::from_str(&json).unwrap();
        assert!(matches!(deser, StopReason::EndTurn));
    }

    #[test]
    fn test_fail_strategy() {
        let strategy = FailStrategy;
        let msgs = vec![types::user("hello")];
        let result = strategy.compact(msgs, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncate_strategy_no_truncation_needed() {
        let strategy = TruncateStrategy { keep_last: 10 };
        let msgs = vec![types::user("a"), types::assistant("b")];
        let result = strategy.compact(msgs.clone(), 100).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_truncate_strategy_keeps_system_and_last() {
        let strategy = TruncateStrategy { keep_last: 2 };
        let msgs = vec![
            types::system("sys"),
            types::user("a"),
            types::assistant("b"),
            types::user("c"),
            types::assistant("d"),
        ];
        let result = strategy.compact(msgs, 100).unwrap();
        // system + last 2
        assert_eq!(result.len(), 3);
        assert!(matches!(&result[0], Message::System(_)));
    }

    #[test]
    fn test_truncate_strategy_no_system() {
        let strategy = TruncateStrategy { keep_last: 2 };
        let msgs = vec![
            types::user("a"),
            types::assistant("b"),
            types::user("c"),
            types::assistant("d"),
        ];
        let result = strategy.compact(msgs, 100).unwrap();
        // Last 2 only (no system to preserve)
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_agent_result_fields() {
        let result = AgentResult {
            messages: vec![types::user("hi")],
            final_message: AssistantMessage {
                content: types::text_content("hello"),
                tool_calls: vec![],
                usage: None,
            },
            total_usage: Usage {
                input_tokens: 10,
                output_tokens: 20,
            },
            turn_count: 1,
            stop_reason: StopReason::EndTurn,
        };
        assert_eq!(result.turn_count, 1);
        assert_eq!(result.total_usage.input_tokens, 10);
    }
}

use rskit_errors::AppError;
use rskit_llm::types::Message;

/// Strategy for compacting the message history when context is too large.
pub trait ContextStrategy: Send + Sync {
    /// Compact the message list toward `max_tokens`.
    ///
    /// Strategies decide how aggressively to reduce history.
    /// Implementations that cannot satisfy their own policy should return an error.
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
///
/// This is a message-count strategy; `max_tokens` is accepted through the common strategy trait
/// but is not used for token-aware trimming.
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
        let has_system = matches!(messages.first(), Some(Message::System(_)));
        let max_messages = self.keep_last + usize::from(has_system);
        if messages.len() <= max_messages {
            return Ok(messages);
        }

        let mut result = Vec::with_capacity(self.keep_last + 1);
        if has_system {
            result.push(messages[0].clone());
        }

        let start = messages.len().saturating_sub(self.keep_last);
        for msg in &messages[start..] {
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
        let result = strategy.compact(msgs, 100).unwrap();
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
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_truncate_strategy_no_system_drops_keep_last_plus_one() {
        let strategy = TruncateStrategy { keep_last: 2 };
        let msgs = vec![types::user("a"), types::assistant("b"), types::user("c")];

        let result = strategy.compact(msgs, 100).unwrap();

        assert_eq!(result.len(), 2);
        assert!(matches!(&result[0], Message::Assistant(_)));
        assert!(matches!(&result[1], Message::User(_)));
    }
}

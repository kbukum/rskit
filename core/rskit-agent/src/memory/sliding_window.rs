use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, ErrorCode};
use rskit_llm::types::Message;

use super::Memory;

// ── SlidingWindowMemory ─────────────────────────────────────────────────────

/// Wraps any [`Memory`] and limits the stored history to the last
/// `max_messages` non-system entries (always preserving a leading system
/// message in addition to that limit).
pub struct SlidingWindowMemory {
    inner: Arc<dyn Memory>,
    max_messages: usize,
}

impl SlidingWindowMemory {
    /// Create a sliding-window wrapper.
    ///
    /// `max_messages` is the number of non-system messages to retain and must
    /// be at least 1.
    pub fn new(inner: Arc<dyn Memory>, max_messages: usize) -> Result<Self, AppError> {
        if max_messages == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "max_messages must be at least 1",
            ));
        }
        Ok(Self {
            inner,
            max_messages,
        })
    }

    /// Trim a message list: keep the system prompt (if present) plus the last
    /// `max_messages` non-system messages.
    fn trim(&self, messages: &[Message]) -> Vec<Message> {
        if messages.len() <= self.max_messages {
            return messages.to_vec();
        }

        let mut result = Vec::with_capacity(self.max_messages + 1);

        let has_system = matches!(messages.first(), Some(Message::System(_)));
        if has_system {
            result.push(messages[0].clone());
        }

        let non_system = if has_system { &messages[1..] } else { messages };
        let start = non_system.len().saturating_sub(self.max_messages);
        result.extend_from_slice(&non_system[start..]);

        result
    }
}

#[async_trait]
impl Memory for SlidingWindowMemory {
    async fn load(&self, session_id: &str) -> Result<Vec<Message>, AppError> {
        let messages = self.inner.load(session_id).await?;
        Ok(self.trim(&messages))
    }

    async fn save(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        let trimmed = self.trim(messages);
        self.inner.save(session_id, &trimmed).await
    }

    async fn append(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        self.inner.append(session_id, messages).await?;
        // Re-load, trim, and persist.
        let all = self.inner.load(session_id).await?;
        let trimmed = self.trim(&all);
        self.inner.save(session_id, &trimmed).await
    }

    async fn clear(&self, session_id: &str) -> Result<(), AppError> {
        self.inner.clear(session_id).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

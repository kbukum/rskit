//! Conversation memory — store and retrieve message history per session.
//!
//! The [`Memory`] trait defines async operations for loading, saving, and
//! appending messages.  [`InMemoryStore`] keeps everything in a
//! `tokio::sync::RwLock<HashMap>`, while [`SlidingWindowMemory`] wraps any
//! `Memory` and trims to a maximum message count.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, ErrorCode};
use rskit_llm::types::Message;
use tokio::sync::RwLock;

// ── Memory trait ────────────────────────────────────────────────────────────

/// Async conversation memory backend.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Load all messages for a session.
    async fn load(&self, session_id: &str) -> Result<Vec<Message>, AppError>;

    /// Replace the entire message list for a session.
    async fn save(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError>;

    /// Append messages to an existing session (creating it if needed).
    async fn append(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError>;

    /// Remove all messages for a session.
    async fn clear(&self, session_id: &str) -> Result<(), AppError>;
}

// ── InMemoryStore ───────────────────────────────────────────────────────────

/// A simple in-process memory store backed by a `RwLock<HashMap>`.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    store: RwLock<HashMap<String, Vec<Message>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Memory for InMemoryStore {
    async fn load(&self, session_id: &str) -> Result<Vec<Message>, AppError> {
        let guard = self.store.read().await;
        Ok(guard.get(session_id).cloned().unwrap_or_default())
    }

    async fn save(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        let mut guard = self.store.write().await;
        guard.insert(session_id.to_string(), messages.to_vec());
        Ok(())
    }

    async fn append(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        let mut guard = self.store.write().await;
        guard
            .entry(session_id.to_string())
            .or_default()
            .extend_from_slice(messages);
        Ok(())
    }

    async fn clear(&self, session_id: &str) -> Result<(), AppError> {
        let mut guard = self.store.write().await;
        guard.remove(session_id);
        Ok(())
    }
}

// ── SlidingWindowMemory ─────────────────────────────────────────────────────

/// Wraps any [`Memory`] and limits the stored history to the last
/// `max_messages` entries (always preserving a leading system message).
pub struct SlidingWindowMemory {
    inner: Arc<dyn Memory>,
    max_messages: usize,
}

impl SlidingWindowMemory {
    /// Create a sliding-window wrapper.
    ///
    /// `max_messages` must be at least 1.
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

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;

    #[tokio::test]
    async fn test_in_memory_store_save_load() {
        let store = InMemoryStore::new();
        let msgs = vec![types::user("hello"), types::assistant("hi")];
        store.save("s1", &msgs).await.unwrap();

        let loaded = store.load("s1").await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_in_memory_store_append() {
        let store = InMemoryStore::new();
        store
            .save("s1", &[types::user("hello")])
            .await
            .unwrap();
        store
            .append("s1", &[types::assistant("hi")])
            .await
            .unwrap();

        let loaded = store.load("s1").await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_in_memory_store_clear() {
        let store = InMemoryStore::new();
        store
            .save("s1", &[types::user("hello")])
            .await
            .unwrap();
        store.clear("s1").await.unwrap();

        let loaded = store.load("s1").await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_store_load_missing() {
        let store = InMemoryStore::new();
        let loaded = store.load("nonexistent").await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_sliding_window_trims() {
        let inner = Arc::new(InMemoryStore::new());
        let sw = SlidingWindowMemory::new(inner.clone(), 2).unwrap();

        let msgs = vec![
            types::user("a"),
            types::assistant("b"),
            types::user("c"),
            types::assistant("d"),
        ];
        sw.save("s1", &msgs).await.unwrap();

        let loaded = sw.load("s1").await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_sliding_window_preserves_system() {
        let inner = Arc::new(InMemoryStore::new());
        let sw = SlidingWindowMemory::new(inner.clone(), 2).unwrap();

        let msgs = vec![
            types::system("sys"),
            types::user("a"),
            types::assistant("b"),
            types::user("c"),
            types::assistant("d"),
        ];
        sw.save("s1", &msgs).await.unwrap();

        let loaded = sw.load("s1").await.unwrap();
        // system + last 2
        assert_eq!(loaded.len(), 3);
        assert!(matches!(&loaded[0], Message::System(_)));
    }

    #[tokio::test]
    async fn test_sliding_window_append_trims() {
        let inner = Arc::new(InMemoryStore::new());
        let sw = SlidingWindowMemory::new(inner.clone(), 3).unwrap();

        sw.save("s1", &[types::user("a"), types::assistant("b")])
            .await
            .unwrap();
        sw.append("s1", &[types::user("c"), types::assistant("d")])
            .await
            .unwrap();

        let loaded = sw.load("s1").await.unwrap();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn test_sliding_window_zero_max_messages() {
        let inner = Arc::new(InMemoryStore::new());
        let result = SlidingWindowMemory::new(inner, 0);
        assert!(result.is_err());
    }
}

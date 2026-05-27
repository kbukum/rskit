//! Conversation memory — store and retrieve message history per session.
//!
//! The [`Memory`] trait defines async operations for loading, saving, and
//! appending messages.  [`InMemoryStore`] keeps everything in a
//! `parking_lot::RwLock<HashMap>`, while [`SlidingWindowMemory`] wraps any
//! `Memory` and trims to a maximum message count.

mod sliding_window;
mod store;

pub use sliding_window::SlidingWindowMemory;
pub use store::InMemoryStore;

use async_trait::async_trait;
use rskit_errors::AppError;
use rskit_llm::types::Message;

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

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types;
    use std::sync::Arc;

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
        store.save("s1", &[types::user("hello")]).await.unwrap();
        store.append("s1", &[types::assistant("hi")]).await.unwrap();

        let loaded = store.load("s1").await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_in_memory_store_clear() {
        let store = InMemoryStore::new();
        store.save("s1", &[types::user("hello")]).await.unwrap();
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

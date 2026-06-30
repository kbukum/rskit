use std::collections::HashMap;

use async_trait::async_trait;
use parking_lot::RwLock;
use rskit_errors::AppError;
use rskit_llm::types::Message;

use super::Memory;

// ── InMemoryStore ───────────────────────────────────────────────────────────

/// A simple in-process memory store backed by a `RwLock<HashMap>`.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    store: RwLock<HashMap<String, Vec<Message>>>,
}

impl InMemoryStore {
    /// Create an empty in-memory message store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Memory for InMemoryStore {
    async fn load(&self, session_id: &str) -> Result<Vec<Message>, AppError> {
        let guard = self.store.read();
        Ok(guard.get(session_id).cloned().unwrap_or_default())
    }

    async fn save(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        let mut guard = self.store.write();
        guard.insert(session_id.to_string(), messages.to_vec());
        drop(guard);
        Ok(())
    }

    async fn append(&self, session_id: &str, messages: &[Message]) -> Result<(), AppError> {
        let mut guard = self.store.write();
        guard
            .entry(session_id.to_string())
            .or_default()
            .extend_from_slice(messages);
        drop(guard);
        Ok(())
    }

    async fn clear(&self, session_id: &str) -> Result<(), AppError> {
        let mut guard = self.store.write();
        guard.remove(session_id);
        drop(guard);
        Ok(())
    }
}

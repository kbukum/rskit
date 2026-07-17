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

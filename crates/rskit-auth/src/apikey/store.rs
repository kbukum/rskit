//! Store trait for API key persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rskit_errors::AppError;

use super::Key;

/// Persistence contract for API keys.
#[async_trait]
pub trait Store: Send + Sync {
    /// Persist a new key record.
    async fn create(&self, key: Key) -> Result<(), AppError>;
    /// List candidate keys by prefix.
    async fn list_by_prefix(&self, key_prefix: &str) -> Result<Vec<Key>, AppError>;
    /// Retrieve a key by ID.
    async fn get_by_id(&self, key_id: &str) -> Result<Key, AppError>;
    /// Update last-used timestamp.
    async fn update_last_used(&self, key_id: &str, used_at: DateTime<Utc>) -> Result<(), AppError>;
    /// Set rotation metadata.
    async fn set_rotation(
        &self,
        key_id: &str,
        grace_ends_at: DateTime<Utc>,
        rotated_by_id: Option<String>,
    ) -> Result<(), AppError>;
    /// Enable or disable a key.
    async fn set_active(&self, key_id: &str, active: bool) -> Result<(), AppError>;
    /// Delete a key.
    async fn delete(&self, key_id: &str) -> Result<(), AppError>;
}

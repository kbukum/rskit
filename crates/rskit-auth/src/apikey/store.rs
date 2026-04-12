//! Store trait for API key persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rskit_errors::AppError;

use super::Key;

/// Persistence contract for API keys. Consumers implement with their database.
#[async_trait]
pub trait Store: Send + Sync {
    /// Persist a new key record.
    async fn create(&self, key: Key) -> Result<(), AppError>;
    /// Retrieve a key by its SHA-256 hash.
    async fn get_by_hash(&self, key_hash: &str) -> Result<Key, AppError>;
    /// Retrieve a key by its unique ID.
    async fn get_by_id(&self, key_id: &str) -> Result<Key, AppError>;
    /// Bump the `last_used_at` timestamp.
    async fn update_last_used(&self, key_id: &str) -> Result<(), AppError>;
    /// Set a grace period on a rotated key.
    async fn set_grace_period(
        &self,
        key_id: &str,
        grace_ends_at: DateTime<Utc>,
        rotated_by_id: Option<String>,
    ) -> Result<(), AppError>;
    /// Enable or disable a key.
    async fn set_active(&self, key_id: &str, active: bool) -> Result<(), AppError>;
    /// Permanently remove a key record.
    async fn delete(&self, key_id: &str) -> Result<(), AppError>;
}

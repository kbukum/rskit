//! Store trait for API key persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rskit_errors::AppError;

use super::Key;

/// Persistence contract for API keys. Consumers implement with their database.
#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, key: Key) -> Result<(), AppError>;
    async fn get_by_hash(&self, key_hash: &str) -> Result<Key, AppError>;
    async fn get_by_id(&self, key_id: &str) -> Result<Key, AppError>;
    async fn update_last_used(&self, key_id: &str) -> Result<(), AppError>;
    async fn set_grace_period(
        &self,
        key_id: &str,
        grace_ends_at: DateTime<Utc>,
        rotated_by_id: Option<String>,
    ) -> Result<(), AppError>;
    async fn set_active(&self, key_id: &str, active: bool) -> Result<(), AppError>;
    async fn delete(&self, key_id: &str) -> Result<(), AppError>;
}

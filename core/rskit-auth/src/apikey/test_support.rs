//! Shared in-memory fixtures for API key manager tests.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rskit_errors::AppError;

use super::{Hasher, HashingConfig, Key, Manager, Store};

/// In-memory [`Store`] backing manager tests.
#[derive(Default)]
pub(crate) struct MemoryStore {
    keys: Mutex<HashMap<String, Key>>,
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, key: Key) -> Result<(), AppError> {
        self.keys.lock().insert(key.id.clone(), key);
        Ok(())
    }

    async fn list_by_prefix(&self, key_prefix: &str) -> Result<Vec<Key>, AppError> {
        Ok(self
            .keys
            .lock()
            .values()
            .filter(|key| key.key_prefix == key_prefix)
            .cloned()
            .collect())
    }

    async fn get_by_id(&self, key_id: &str) -> Result<Key, AppError> {
        self.keys
            .lock()
            .get(key_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("API key", Some(key_id)))
    }

    async fn update_last_used(&self, key_id: &str, used_at: DateTime<Utc>) -> Result<(), AppError> {
        if let Some(key) = self.keys.lock().get_mut(key_id) {
            key.last_used_at = Some(used_at);
        }
        Ok(())
    }

    async fn set_rotation(
        &self,
        key_id: &str,
        grace_ends_at: DateTime<Utc>,
        rotated_by_id: Option<String>,
    ) -> Result<(), AppError> {
        if let Some(key) = self.keys.lock().get_mut(key_id) {
            key.grace_ends_at = Some(grace_ends_at);
            key.rotated_by_id = rotated_by_id;
        }
        Ok(())
    }

    async fn set_active(&self, key_id: &str, active: bool) -> Result<(), AppError> {
        if let Some(key) = self.keys.lock().get_mut(key_id) {
            key.is_active = active;
        }
        Ok(())
    }

    async fn delete(&self, key_id: &str) -> Result<(), AppError> {
        self.keys.lock().remove(key_id);
        Ok(())
    }
}

/// Build a manager over a fresh in-memory store with a deterministic hasher.
pub(crate) fn manager() -> Manager<MemoryStore> {
    Manager::new(
        MemoryStore::default(),
        Hasher::new(HashingConfig {
            pepper: "p".repeat(32),
            entropy_bytes: 32,
        })
        .unwrap(),
    )
}

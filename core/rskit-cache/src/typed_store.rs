use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::registry::CacheBackend;

/// A generic, JSON-serialised store backed by a [`CacheBackend`].
///
/// Keys are automatically prefixed with the store's `prefix` so that
/// multiple `TypedStore` instances can coexist on the same backend without
/// key collisions.
pub struct TypedStore<T> {
    client: Arc<dyn CacheBackend>,
    prefix: String,
    _marker: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync> TypedStore<T> {
    /// Create a new typed store that prefixes all keys with `prefix`.
    pub fn new(client: Arc<dyn CacheBackend>, prefix: impl Into<String>) -> Self {
        Self {
            client,
            prefix: prefix.into(),
            _marker: PhantomData,
        }
    }

    /// Build the backend key used for storage.
    fn full_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix, key)
    }

    /// Retrieve a value by key, deserialising from JSON.
    pub async fn get(&self, key: &str) -> AppResult<Option<T>> {
        let raw = self.client.get(&self.full_key(key)).await?;
        match raw {
            Some(json) => {
                let val = serde_json::from_str(&json).map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("json deserialise error: {e}"))
                        .with_cause(e)
                })?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// Store a value by key, serialising to JSON. An optional TTL may be set.
    pub async fn set(&self, key: &str, val: &T, ttl: Option<Duration>) -> AppResult<()> {
        let json = serde_json::to_string(val).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("json serialise error: {e}")).with_cause(e)
        })?;
        self.client.set(&self.full_key(key), &json, ttl).await
    }

    /// Delete a key. Returns `true` if the key existed.
    pub async fn delete(&self, key: &str) -> AppResult<bool> {
        self.client.delete(&self.full_key(key)).await
    }

    /// Check whether a key exists.
    pub async fn exists(&self, key: &str) -> AppResult<bool> {
        self.client.exists(&self.full_key(key)).await
    }
}

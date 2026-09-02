use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::registry::CacheStore;

/// A generic, JSON-serialised store backed by a [`CacheStore`].
///
/// Keys are automatically prefixed with the store's `prefix`
/// so that multiple `TypedStore` instances can coexist on the same cache store without key collisions.
pub struct TypedStore<T> {
    client: Arc<dyn CacheStore>,
    prefix: String,
    _marker: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync> TypedStore<T> {
    /// Create a new typed store that prefixes all keys with `prefix`.
    pub fn new(client: Arc<dyn CacheStore>, prefix: impl Into<String>) -> Self {
        Self {
            client,
            prefix: prefix.into(),
            _marker: PhantomData,
        }
    }

    /// Build the cache-store key used for storage.
    fn full_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix, key)
    }

    /// Retrieve a value by key, deserialising from JSON.
    pub async fn get(&self, key: &str) -> AppResult<Option<T>> {
        let raw = self.client.get(&self.full_key(key)).await?;
        match raw {
            Some(bytes) => {
                let val = serde_json::from_slice(&bytes).map_err(|e| {
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
        let json = serde_json::to_vec(val).map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use serde::Serializer;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryStore {
        values: Mutex<BTreeMap<String, Vec<u8>>>,
    }

    #[async_trait::async_trait]
    impl CacheStore for MemoryStore {
        async fn get(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
            Ok(self.values.lock().get(key).cloned())
        }

        async fn set(&self, key: &str, val: &[u8], _ttl: Option<Duration>) -> AppResult<()> {
            self.values.lock().insert(key.to_string(), val.to_vec());
            Ok(())
        }

        async fn delete(&self, key: &str) -> AppResult<bool> {
            Ok(self.values.lock().remove(key).is_some())
        }

        async fn exists(&self, key: &str) -> AppResult<bool> {
            Ok(self.values.lock().contains_key(key))
        }
    }

    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom("boom"))
        }
    }

    impl<'de> serde::Deserialize<'de> for FailingSerialize {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(Self)
        }
    }

    #[tokio::test]
    async fn typed_store_round_trips_prefixed_json() {
        let store = Arc::new(MemoryStore::default());
        let typed = TypedStore::<u32>::new(store.clone(), "numbers");

        typed
            .set("answer", &42, None)
            .await
            .expect("set should serialise");

        assert!(store.exists("numbers:answer").await.expect("exists works"));
        assert_eq!(
            typed.get("answer").await.expect("get should deserialise"),
            Some(42)
        );
        assert!(typed.delete("answer").await.expect("delete should succeed"));
        assert_eq!(
            typed.get("answer").await.expect("missing key succeeds"),
            None
        );
    }

    #[tokio::test]
    async fn get_rejects_invalid_json() {
        let store = Arc::new(MemoryStore::default());
        store
            .set("numbers:bad", b"not-json", None)
            .await
            .expect("fixture write succeeds");
        let typed = TypedStore::<u32>::new(store, "numbers");

        let err = typed
            .get("bad")
            .await
            .expect_err("invalid json should fail");
        assert_eq!(err.code(), ErrorCode::Internal);
    }

    #[tokio::test]
    async fn set_rejects_serialisation_errors() {
        let store = Arc::new(MemoryStore::default());
        let typed = TypedStore::<FailingSerialize>::new(store.clone(), "bad");

        let err = typed
            .set("value", &FailingSerialize, None)
            .await
            .expect_err("serialisation errors should surface");
        assert_eq!(err.code(), ErrorCode::Internal);

        store
            .set("bad:value", b"null", None)
            .await
            .expect("fixture write succeeds");
        assert!(
            typed
                .get("value")
                .await
                .expect("deserialise should succeed")
                .is_some()
        );
    }
}

//! Filesystem cache adapter.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::{CacheBackend, CacheConfig, CacheFactory, CacheRegistry};

/// Filesystem cache configuration.
///
/// The root path is supplied when registering the adapter because it is a
/// deployment concern owned by the composition boundary, not common cache
/// selection configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileCacheConfig {
    /// Root directory used to store cache entries.
    pub root: PathBuf,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
}

impl FileCacheConfig {
    /// Create filesystem cache configuration rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            key_prefix: None,
        }
    }
}

/// Persistent filesystem cache adapter.
pub struct FileCache {
    config: FileCacheConfig,
}

impl FileCache {
    /// Create a filesystem cache adapter.
    #[must_use]
    pub fn new(config: FileCacheConfig) -> Self {
        Self { config }
    }

    fn prefixed_key(&self, key: &str) -> String {
        self.config
            .key_prefix
            .as_ref()
            .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        let hash = blake3::hash(key.as_bytes()).to_hex().to_string();
        self.config.root.join(&hash[..2]).join(hash)
    }

    async fn read_entry(&self, path: &Path, expected_key: &str) -> AppResult<Option<Entry>> {
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("failed to read cache entry '{}'", path.display()),
                )
                .with_cause(error));
            }
        };
        let entry: Entry = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to decode cache entry '{}'", path.display()),
            )
            .with_cause(error)
        })?;
        if entry.key != expected_key {
            return Err(AppError::new(
                ErrorCode::Conflict,
                format!("cache key collision for '{}'", path.display()),
            ));
        }
        if entry.is_expired()? {
            return Ok(None);
        }
        Ok(Some(entry))
    }
}

#[async_trait::async_trait]
impl CacheBackend for FileCache {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let key = self.prefixed_key(key);
        let path = self.entry_path(&key);
        Ok(self.read_entry(&path, &key).await?.map(|entry| entry.value))
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        if ttl.is_some_and(|ttl| ttl.is_zero()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache TTL must be greater than zero",
            ));
        }
        let key = self.prefixed_key(key);
        let path = self.entry_path(&key);
        let entry = Entry {
            key,
            value: val.to_owned(),
            expires_at_millis: expires_at_millis(ttl)?,
        };
        let json = serde_json::to_vec(&entry).map_err(|error| {
            AppError::new(ErrorCode::Internal, "failed to encode cache entry").with_cause(error)
        })?;
        rskit_fs::async_io::file::write_atomic_replace(&path, json, "rskit-cache").await
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let key = self.prefixed_key(key);
        let path = self.entry_path(&key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to delete cache entry '{}'", path.display()),
            )
            .with_cause(error)),
        }
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        self.get(key).await.map(|value| value.is_some())
    }
}

struct FileFactory {
    config: FileCacheConfig,
}

#[async_trait::async_trait]
impl CacheFactory for FileFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheBackend>> {
        let mut fs = self.config.clone();
        if fs.key_prefix.is_none() {
            fs.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(Arc::new(FileCache::new(fs)))
    }
}

/// Explicitly register the filesystem adapter.
pub fn register_file_cache(registry: &mut CacheRegistry, config: FileCacheConfig) -> AppResult<()> {
    registry.register("fs", Arc::new(FileFactory { config }))
}

#[derive(Deserialize, Serialize)]
struct Entry {
    key: String,
    value: String,
    expires_at_millis: Option<u128>,
}

impl Entry {
    fn is_expired(&self) -> AppResult<bool> {
        self.expires_at_millis
            .map(|expires_at| now_millis().map(|now| expires_at <= now))
            .transpose()
            .map(|expired| expired.unwrap_or(false))
    }
}

fn expires_at_millis(ttl: Option<Duration>) -> AppResult<Option<u128>> {
    ttl.map(ttl_millis)
        .transpose()?
        .map(|ttl| now_millis().and_then(|now| now.checked_add(ttl).ok_or_else(ttl_error)))
        .transpose()
}

fn ttl_millis(ttl: Duration) -> AppResult<u128> {
    if ttl.is_zero() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "cache TTL must be greater than zero",
        ));
    }
    Ok(ttl.as_millis().max(1))
}

fn now_millis() -> AppResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "system clock is before UNIX_EPOCH")
                .with_cause(error)
        })
}

fn ttl_error() -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        "cache TTL is too large to represent safely for filesystem cache",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[tokio::test]
    async fn stores_and_reads_values() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));

        cache.set("key", "value", None).await.unwrap();

        assert_eq!(cache.get("key").await.unwrap().as_deref(), Some("value"));
        assert!(cache.exists("key").await.unwrap());
        assert!(cache.delete("key").await.unwrap());
        assert!(!cache.exists("key").await.unwrap());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn updates_existing_values() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));

        cache.set("key", "old", None).await.unwrap();
        cache.set("key", "new", None).await.unwrap();

        assert_eq!(cache.get("key").await.unwrap().as_deref(), Some("new"));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_zero_ttl() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));

        let err = cache
            .set("key", "value", Some(Duration::ZERO))
            .await
            .expect_err("zero TTL must be rejected");

        assert_eq!(err.code, ErrorCode::InvalidInput);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn expired_entries_miss_without_deleting_from_read_path() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));
        let key = cache.prefixed_key("key");
        let path = cache.entry_path(&key);
        let entry = Entry {
            key,
            value: "value".to_owned(),
            expires_at_millis: Some(now_millis().unwrap().saturating_sub(1)),
        };

        rskit_fs::async_io::file::write_atomic_replace(
            &path,
            serde_json::to_vec(&entry).unwrap(),
            "rskit-cache-test",
        )
        .await
        .unwrap();

        assert_eq!(cache.get("key").await.unwrap(), None);
        assert!(!cache.exists("key").await.unwrap());
        assert!(tokio::fs::metadata(path).await.is_ok());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn positive_sub_millisecond_ttl_rounds_up() {
        assert_eq!(ttl_millis(Duration::from_nanos(1)).unwrap(), 1);
    }

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "rskit-cache-fs-{}-{}-{}",
            std::process::id(),
            now_millis().unwrap(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}

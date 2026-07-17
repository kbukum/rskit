use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::CacheStore;

use super::FileCacheConfig;

/// Persistent filesystem cache adapter.
pub struct FileCache {
    config: FileCacheConfig,
    mutation_lock: Arc<Mutex<()>>,
}

impl FileCache {
    /// Create a filesystem cache adapter.
    #[must_use]
    pub fn new(config: FileCacheConfig) -> Self {
        Self {
            config,
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn prefixed_key(&self, key: &str) -> String {
        self.config
            .key_prefix
            .as_ref()
            .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
    }

    pub(crate) fn entry_path(&self, key: &str) -> PathBuf {
        let hash = rskit_util::hash::hash_hex(key.as_bytes());
        self.config.root.join(&hash[..2]).join(hash)
    }

    async fn read_entry(&self, path: &Path, expected_key: &str) -> AppResult<Option<Entry>> {
        let Some(entry) = self.read_entry_file(path).await? else {
            return Ok(None);
        };
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

    async fn read_entry_file(&self, path: &Path) -> AppResult<Option<Entry>> {
        let bytes =
            match rskit_fs::async_io::file::read_bounded(path, self.config.max_entry_bytes).await {
                Ok(bytes) => bytes,
                Err(error) if is_not_found_error(&error) => return Ok(None),
                Err(error) => return Err(error),
            };
        let entry: Entry = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to decode cache entry '{}'", path.display()),
            )
            .with_cause(error)
        })?;
        Ok(Some(entry))
    }

    /// Remove expired cache entries, checking at most `max_entries` files.
    ///
    /// Reads stay non-destructive to avoid unlinking a concurrent fresh write.
    /// Call this method from application-owned maintenance code when filesystem
    /// cache entries use TTLs and the cache directory needs bounded cleanup.
    pub async fn cleanup_expired(&self, max_entries: usize) -> AppResult<usize> {
        if max_entries == 0 {
            return Ok(0);
        }

        let _guard = self.mutation_lock.lock().await;
        let mut checked = 0;
        let mut removed = 0;
        if !rskit_fs::async_io::dir::exists(&self.config.root).await? {
            return Ok(0);
        }
        let shard_dirs = rskit_fs::async_io::dir::list(&self.config.root).await?;

        for shard in shard_dirs.into_iter().filter(|entry| entry.is_dir) {
            let entries = match rskit_fs::async_io::dir::list(&shard.path).await {
                Ok(entries) => entries,
                Err(error) if is_not_found_error(&error) => continue,
                Err(error) => return Err(error),
            };
            for entry in entries.into_iter().filter(|entry| entry.is_file) {
                if checked == max_entries {
                    return Ok(removed);
                }
                checked += 1;
                let Some(cache_entry) = self.read_entry_file(&entry.path).await? else {
                    continue;
                };
                if cache_entry.is_expired()? {
                    match tokio::fs::remove_file(&entry.path).await {
                        Ok(()) => removed += 1,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            return Err(AppError::new(
                                ErrorCode::Internal,
                                format!(
                                    "failed to delete expired cache entry '{}'",
                                    entry.path.display()
                                ),
                            )
                            .with_cause(error));
                        }
                    }
                }
            }
        }

        Ok(removed)
    }
}

#[async_trait::async_trait]
impl CacheStore for FileCache {
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
        if json.len() as u64 > self.config.max_entry_bytes {
            return Err(cache_entry_too_large_error(
                &path,
                json.len() as u64,
                self.config.max_entry_bytes,
            ));
        }
        let _guard = self.mutation_lock.lock().await;
        rskit_fs::async_io::file::write_atomic_replace(&path, json, "rskit-cache").await
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let key = self.prefixed_key(key);
        let path = self.entry_path(&key);
        let _guard = self.mutation_lock.lock().await;
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

#[derive(Deserialize, Serialize)]
pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) expires_at_millis: Option<u128>,
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

pub(crate) fn ttl_millis(ttl: Duration) -> AppResult<u128> {
    if ttl.is_zero() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "cache TTL must be greater than zero",
        ));
    }
    Ok(ttl.as_millis().max(1))
}

pub(crate) fn now_millis() -> AppResult<u128> {
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

fn is_not_found_error(error: &AppError) -> bool {
    error
        .cause()
        .and_then(|cause| cause.downcast_ref::<std::io::Error>())
        .is_some_and(|cause| cause.kind() == std::io::ErrorKind::NotFound)
}

fn cache_entry_too_large_error(path: &Path, actual: u64, limit: u64) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "cache entry '{}' is {actual} bytes, exceeding limit {limit} bytes",
            path.display()
        ),
    )
    .with_detail("rskit_cache_error", "entry_too_large")
    .with_detail("actual_bytes", actual)
    .with_detail("limit_bytes", limit)
}

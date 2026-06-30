//! Filesystem cache adapter.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{CacheConfig, CacheRegistry, CacheStore, CacheStoreFactory};

const DEFAULT_MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

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
    /// Maximum serialized cache-entry size accepted by reads and writes.
    #[serde(default = "default_max_entry_bytes")]
    pub max_entry_bytes: u64,
}

impl FileCacheConfig {
    /// Create filesystem cache configuration rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            key_prefix: None,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
        }
    }
}

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

    fn prefixed_key(&self, key: &str) -> String {
        self.config
            .key_prefix
            .as_ref()
            .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
    }

    fn entry_path(&self, key: &str) -> PathBuf {
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

struct FileFactory {
    config: FileCacheConfig,
}

#[async_trait::async_trait]
impl CacheStoreFactory for FileFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
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

const fn default_max_entry_bytes() -> u64 {
    DEFAULT_MAX_ENTRY_BYTES
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
    async fn registry_build_inherits_global_key_prefix() {
        let root = temp_root();
        let mut registry = CacheRegistry::new();
        register_file_cache(&mut registry, FileCacheConfig::new(&root)).unwrap();
        let cache = registry
            .build(&cache_config_with_prefix("global"))
            .await
            .unwrap();

        cache.set("key", "value", None).await.unwrap();

        let mut global_config = FileCacheConfig::new(&root);
        global_config.key_prefix = Some("global".to_owned());
        assert_eq!(
            FileCache::new(global_config)
                .get("key")
                .await
                .unwrap()
                .as_deref(),
            Some("value")
        );
        assert_eq!(
            FileCache::new(FileCacheConfig::new(&root))
                .get("key")
                .await
                .unwrap(),
            None
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn registry_build_preserves_adapter_key_prefix_override() {
        let root = temp_root();
        let mut registry = CacheRegistry::new();
        let mut adapter_config = FileCacheConfig::new(&root);
        adapter_config.key_prefix = Some("adapter".to_owned());
        register_file_cache(&mut registry, adapter_config).unwrap();
        let cache = registry
            .build(&cache_config_with_prefix("global"))
            .await
            .unwrap();

        cache.set("key", "value", None).await.unwrap();

        let mut adapter_reader_config = FileCacheConfig::new(&root);
        adapter_reader_config.key_prefix = Some("adapter".to_owned());
        let mut global_reader_config = FileCacheConfig::new(&root);
        global_reader_config.key_prefix = Some("global".to_owned());
        assert_eq!(
            FileCache::new(adapter_reader_config)
                .get("key")
                .await
                .unwrap()
                .as_deref(),
            Some("value")
        );
        assert_eq!(
            FileCache::new(global_reader_config)
                .get("key")
                .await
                .unwrap(),
            None
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_entries_exceeding_configured_size() {
        let root = temp_root();
        let mut config = FileCacheConfig::new(&root);
        config.max_entry_bytes = 8;
        let cache = FileCache::new(config);

        let err = cache
            .set("key", "value larger than limit", None)
            .await
            .expect_err("oversized entries must be rejected");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_oversized_entry_files_before_decoding() {
        let root = temp_root();
        let mut config = FileCacheConfig::new(&root);
        config.max_entry_bytes = 8;
        let cache = FileCache::new(config);
        let key = cache.prefixed_key("key");
        let path = cache.entry_path(&key);

        rskit_fs::async_io::file::write_atomic_replace(&path, b"012345678", "rskit-cache-test")
            .await
            .unwrap();

        let err = cache
            .get("key")
            .await
            .expect_err("oversized entry files must be rejected");

        assert_eq!(err.code(), ErrorCode::InvalidInput);
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

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn expired_entries_miss_without_deleting_from_read_path() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));
        let key = cache.prefixed_key("key");
        let path = cache.entry_path(&key);
        let entry = expired_entry(key, "value");

        write_entry(&path, &entry).await;

        assert_eq!(cache.get("key").await.unwrap(), None);
        assert!(!cache.exists("key").await.unwrap());
        assert!(tokio::fs::metadata(path).await.is_ok());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn cleanup_expired_removes_expired_entries_without_deleting_live_entries() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));
        let expired_key = cache.prefixed_key("expired");
        let expired_path = cache.entry_path(&expired_key);
        let live_key = cache.prefixed_key("live");
        let live_path = cache.entry_path(&live_key);

        write_entry(&expired_path, &expired_entry(expired_key, "expired")).await;
        write_entry(
            &live_path,
            &Entry {
                key: live_key,
                value: "live".to_owned(),
                expires_at_millis: Some(now_millis().unwrap() + 60_000),
            },
        )
        .await;

        assert_eq!(cache.cleanup_expired(16).await.unwrap(), 1);
        assert!(tokio::fs::metadata(expired_path).await.is_err());
        assert!(tokio::fs::metadata(live_path).await.is_ok());
        assert_eq!(cache.get("live").await.unwrap().as_deref(), Some("live"));
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn cleanup_expired_checks_at_most_requested_entries() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));
        let first_key = cache.prefixed_key("first");
        let first_path = cache.entry_path(&first_key);
        let second_key = cache.prefixed_key("second");
        let second_path = cache.entry_path(&second_key);

        write_entry(&first_path, &expired_entry(first_key, "first")).await;
        write_entry(&second_path, &expired_entry(second_key, "second")).await;

        assert_eq!(cache.cleanup_expired(1).await.unwrap(), 1);
        let remaining = [first_path, second_path]
            .into_iter()
            .filter(|path| path.exists())
            .count();
        assert_eq!(remaining, 1);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn cleanup_expired_returns_zero_when_cache_root_is_missing() {
        let root = temp_root();
        let cache = FileCache::new(FileCacheConfig::new(&root));

        assert_eq!(cache.cleanup_expired(16).await.unwrap(), 0);
    }

    #[test]
    fn positive_sub_millisecond_ttl_rounds_up() {
        assert_eq!(ttl_millis(Duration::from_nanos(1)).unwrap(), 1);
    }

    #[test]
    fn new_config_uses_default_entry_size_limit() {
        assert_eq!(
            FileCacheConfig::new("cache").max_entry_bytes,
            DEFAULT_MAX_ENTRY_BYTES
        );
    }

    fn cache_config_with_prefix(prefix: &str) -> CacheConfig {
        CacheConfig {
            store: "fs".to_owned(),
            key_prefix: Some(prefix.to_owned()),
            ..CacheConfig::default()
        }
    }

    fn expired_entry(key: String, value: &str) -> Entry {
        Entry {
            key,
            value: value.to_owned(),
            expires_at_millis: Some(now_millis().unwrap().saturating_sub(1)),
        }
    }

    async fn write_entry(path: &Path, entry: &Entry) {
        rskit_fs::async_io::file::write_atomic_replace(
            path,
            serde_json::to_vec(entry).unwrap(),
            "rskit-cache-test",
        )
        .await
        .unwrap();
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

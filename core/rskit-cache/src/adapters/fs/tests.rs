use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rskit_errors::ErrorCode;

use crate::{CacheConfig, CacheRegistry, CacheStore};

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

#[cfg(unix)]
#[tokio::test]
async fn cleanup_expired_surfaces_delete_errors() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root();
    let cache = FileCache::new(FileCacheConfig::new(&root));
    let key = cache.prefixed_key("expired");
    let path = cache.entry_path(&key);
    write_entry(&path, &expired_entry(key, "expired")).await;

    let shard = path.parent().unwrap().to_path_buf();
    let original = tokio::fs::metadata(&shard).await.unwrap().permissions();
    // A read-only shard directory lets the entry be read but blocks unlinking,
    // exercising the non-`NotFound` delete failure path.
    tokio::fs::set_permissions(&shard, std::fs::Permissions::from_mode(0o500))
        .await
        .unwrap();

    let result = cache.cleanup_expired(16).await;

    tokio::fs::set_permissions(&shard, original).await.unwrap();
    let _ = tokio::fs::remove_dir_all(&root).await;

    // A privileged user bypasses directory permissions, so the delete succeeds
    // and the error path is unreachable; only assert it when the block held.
    if let Err(error) = result {
        assert_eq!(error.code(), ErrorCode::Internal);
    }
}

#[test]
fn positive_sub_millisecond_ttl_rounds_up() {
    assert_eq!(ttl_millis(Duration::from_nanos(1)).unwrap(), 1);
}

#[test]
fn ttl_millis_rejects_zero_duration() {
    assert_eq!(
        ttl_millis(Duration::ZERO).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn config_applies_default_entry_size_limit_when_omitted() {
    let config: FileCacheConfig =
        serde_json::from_value(serde_json::json!({ "root": "cache", "key_prefix": null })).unwrap();
    assert_eq!(config.max_entry_bytes, DEFAULT_MAX_ENTRY_BYTES);
}

#[tokio::test]
async fn get_reports_key_collision() {
    let root = temp_root();
    let cache = FileCache::new(FileCacheConfig::new(&root));
    let key = cache.prefixed_key("key");
    let path = cache.entry_path(&key);
    write_entry(
        &path,
        &Entry {
            key: "different".to_owned(),
            value: "value".to_owned(),
            expires_at_millis: None,
        },
    )
    .await;

    let err = cache
        .get("key")
        .await
        .expect_err("colliding keys must be rejected");
    assert_eq!(err.code(), ErrorCode::Conflict);
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn get_rejects_corrupt_entry_file() {
    let root = temp_root();
    let cache = FileCache::new(FileCacheConfig::new(&root));
    let key = cache.prefixed_key("key");
    let path = cache.entry_path(&key);
    rskit_fs::async_io::file::write_atomic_replace(&path, b"{not valid json", "rskit-cache-test")
        .await
        .unwrap();

    let err = cache
        .get("key")
        .await
        .expect_err("corrupt entry files must be rejected");
    assert_eq!(err.code(), ErrorCode::Internal);
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn cleanup_expired_returns_zero_when_limit_is_zero() {
    let root = temp_root();
    let cache = FileCache::new(FileCacheConfig::new(&root));
    let key = cache.prefixed_key("expired");
    write_entry(&cache.entry_path(&key), &expired_entry(key, "expired")).await;

    assert_eq!(cache.cleanup_expired(0).await.unwrap(), 0);
    let _ = tokio::fs::remove_dir_all(root).await;
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

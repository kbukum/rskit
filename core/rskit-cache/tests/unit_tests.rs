use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_cache::{
    CacheConfig, CacheRegistry, CacheStore, MemoryCache, MemoryConfig, TypedStore, register_memory,
};

#[test]
fn registry_is_empty_without_explicit_registration() {
    let registry = CacheRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.contains("memory"));
}

#[tokio::test]
async fn explicit_memory_registration_builds_store() {
    let mut registry = CacheRegistry::new();
    register_memory(&mut registry).unwrap();

    let cache = registry.build(&CacheConfig::default()).await.unwrap();
    cache.set("hello", "world", None).await.unwrap();

    assert_eq!(cache.get("hello").await.unwrap().as_deref(), Some("world"));
}

#[tokio::test]
async fn unregistered_store_returns_error() {
    let registry = CacheRegistry::new();
    let err = registry.build(&CacheConfig::default()).await.err().unwrap();
    assert!(err.to_string().contains("not registered"));
}

#[tokio::test]
async fn memory_cache_ttl_zero_is_invalid() {
    let cache = MemoryCache::default();
    let err = cache
        .set("boundary", "value", Some(Duration::ZERO))
        .await
        .unwrap_err();

    assert!(err.to_string().contains("TTL must be greater than zero"));
}

#[tokio::test]
async fn memory_cache_ttl_boundary_expires_after_duration() {
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());
    cache
        .set("short", "value", Some(Duration::from_millis(1)))
        .await
        .unwrap();
    *now.lock() += Duration::from_millis(5);

    assert_eq!(cache.get("short").await.unwrap(), None);
}

#[tokio::test]
async fn memory_cache_zero_capacity_is_unbounded() {
    let cache = MemoryCache::new(None, Some(0));
    cache.set("first", "value", None).await.unwrap();
    cache.set("second", "value", None).await.unwrap();

    assert_eq!(cache.get("first").await.unwrap().as_deref(), Some("value"));
    assert_eq!(cache.get("second").await.unwrap().as_deref(), Some("value"));
}

#[tokio::test]
async fn typed_store_round_trips_json_via_cache_trait() {
    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct Session {
        user_id: String,
        roles: Vec<String>,
    }

    let cache: Arc<dyn CacheStore> = Arc::new(MemoryCache::default());
    let store = TypedStore::<Session>::new(cache, "sessions");

    let expected = Session {
        user_id: "u1".into(),
        roles: vec!["admin".into()],
    };
    store.set("s1", &expected, None).await.unwrap();

    assert_eq!(store.get("s1").await.unwrap(), Some(expected));
    assert!(store.exists("s1").await.unwrap());
    assert!(store.delete("s1").await.unwrap());
    assert!(!store.exists("s1").await.unwrap());
}

#[test]
fn config_defaults_to_memory_store() {
    let cfg = CacheConfig::default();
    assert_eq!(cfg.store, "memory");
    assert!(cfg.key_prefix.is_none());
    assert!(cfg.memory.max_entries.is_none());
}

#[test]
fn deserialise_cache_config_with_memory_options() {
    let cfg: CacheConfig = serde_json::from_str(
        r#"{"store":"memory","key_prefix":"app","memory":{"max_entries":32}}"#,
    )
    .unwrap();

    assert_eq!(cfg.store, "memory");
    assert_eq!(cfg.key_prefix.as_deref(), Some("app"));
    assert_eq!(cfg.memory.max_entries, Some(32));
}

#[test]
fn memory_config_default_is_unbounded() {
    assert!(MemoryConfig::default().max_entries.is_none());
}

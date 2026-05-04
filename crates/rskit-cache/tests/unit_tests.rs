use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_cache::{
    CacheBackend, CacheConfig, CacheRegistry, MemoryCache, MemoryConfig, TypedStore,
    register_memory,
};

#[test]
fn registry_is_empty_without_explicit_registration() {
    let registry = CacheRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.contains("memory"));
}

#[tokio::test]
async fn explicit_memory_registration_builds_backend() {
    let mut registry = CacheRegistry::new();
    register_memory(&mut registry).unwrap();

    let cache = registry.build(&CacheConfig::default()).await.unwrap();
    cache.set("hello", "world", None).await.unwrap();

    assert_eq!(cache.get("hello").await.unwrap().as_deref(), Some("world"));
}

#[tokio::test]
async fn unregistered_backend_returns_error() {
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

    let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCache::default());
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
fn config_defaults_to_memory_backend() {
    let cfg = CacheConfig::default();
    assert_eq!(cfg.backend, "memory");
    assert!(cfg.key_prefix.is_none());
    assert!(cfg.memory.max_entries.is_none());
}

#[test]
fn deserialise_cache_config_with_memory_options() {
    let cfg: CacheConfig = serde_json::from_str(
        r#"{"backend":"memory","key_prefix":"app","memory":{"max_entries":32}}"#,
    )
    .unwrap();

    assert_eq!(cfg.backend, "memory");
    assert_eq!(cfg.key_prefix.as_deref(), Some("app"));
    assert_eq!(cfg.memory.max_entries, Some(32));
}

#[test]
fn memory_config_default_is_unbounded() {
    assert!(MemoryConfig::default().max_entries.is_none());
}

#[cfg(feature = "redis")]
mod redis_feature {
    use std::time::Duration;

    use rskit_cache::{CacheConfig, CacheRegistry, RedisConfig, register_redis};

    #[test]
    fn redis_config_connection_url_no_password() {
        let cfg = RedisConfig::default();
        assert_eq!(cfg.connection_url(), "redis://127.0.0.1:6379/0");
    }

    #[test]
    fn redis_config_connection_url_with_password() {
        let cfg = RedisConfig {
            password: Some("s3cret".into()),
            ..Default::default()
        };
        assert_eq!(cfg.connection_url(), "redis://:s3cret@127.0.0.1:6379/0");
    }

    #[test]
    fn deserialise_redis_config_nested_in_cache_config() {
        let cfg: CacheConfig = serde_json::from_str(
            r#"{"backend":"redis","redis":{"host":"localhost","port":6380,"database":2,"connect_timeout":10}}"#,
        )
        .unwrap();
        assert_eq!(cfg.redis.host, "localhost");
        assert_eq!(cfg.redis.port, 6380);
        assert_eq!(cfg.redis.database, 2);
        assert_eq!(cfg.redis.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn redis_registration_is_explicit() {
        let mut registry = CacheRegistry::new();
        assert!(!registry.contains("redis"));
        register_redis(&mut registry).unwrap();
        assert!(registry.contains("redis"));
    }
}

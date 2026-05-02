use std::time::Duration;

use rskit_cache::{CacheBackend, MemoryCache};

#[tokio::test]
async fn memory_cache_delete_reports_existence() {
    let cache = MemoryCache::default();

    assert!(!cache.delete("missing").await.unwrap());
    cache.set("key", "value", None).await.unwrap();
    assert!(cache.delete("key").await.unwrap());
    assert!(!cache.delete("key").await.unwrap());
}

#[tokio::test]
async fn memory_cache_prefix_isolated() {
    let a = MemoryCache::new(Some("a".into()), None);
    let b = MemoryCache::new(Some("b".into()), None);

    a.set("same", "one", None).await.unwrap();
    b.set("same", "two", None).await.unwrap();

    assert_eq!(a.get("same").await.unwrap().as_deref(), Some("one"));
    assert_eq!(b.get("same").await.unwrap().as_deref(), Some("two"));
}

#[tokio::test]
async fn memory_cache_prunes_expired_before_capacity_eviction() {
    let cache = MemoryCache::new(None, Some(1));

    cache
        .set("expired", "old", Some(Duration::from_millis(1)))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    cache.set("fresh", "new", None).await.unwrap();

    assert_eq!(cache.get("expired").await.unwrap(), None);
    assert_eq!(cache.get("fresh").await.unwrap().as_deref(), Some("new"));
}

#[cfg(feature = "redis")]
mod redis_integration {
    use rskit_cache::{RedisClient, RedisConfig};

    #[tokio::test]
    #[ignore = "requires running Redis server"]
    async fn client_set_and_get() {
        let cfg = RedisConfig::default();
        let client = RedisClient::new(cfg).await.unwrap();
        client.set("test_key", "hello", None).await.unwrap();
        let val = client.get("test_key").await.unwrap();
        assert_eq!(val.as_deref(), Some("hello"));
        client.delete("test_key").await.unwrap();
    }
}

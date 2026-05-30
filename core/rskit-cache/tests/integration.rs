use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rskit_cache::{CacheStore as _, MemoryCache};

#[tokio::test]
async fn memory_cache_delete_reports_existence() {
    let cache = MemoryCache::default();

    assert!(!cache.delete("missing").await.unwrap());
    cache.set("key", "value", None).await.unwrap();
    assert!(cache.delete("key").await.unwrap());
    assert!(!cache.delete("key").await.unwrap());
}

#[tokio::test]
async fn memory_cache_delete_reports_false_for_expired_key() {
    let now = Arc::new(Mutex::new(std::time::Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());

    cache
        .set("expired", "value", Some(Duration::from_millis(1)))
        .await
        .unwrap();
    *now.lock() += Duration::from_millis(5);

    assert!(!cache.delete("expired").await.unwrap());
    assert_eq!(cache.get("expired").await.unwrap(), None);
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
    let now = Arc::new(Mutex::new(std::time::Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, Some(1), move || *clock.lock());

    cache
        .set("expired", "old", Some(Duration::from_millis(1)))
        .await
        .unwrap();
    *now.lock() += Duration::from_millis(5);
    cache.set("fresh", "new", None).await.unwrap();

    assert_eq!(cache.get("expired").await.unwrap(), None);
    assert_eq!(cache.get("fresh").await.unwrap().as_deref(), Some("new"));
}

#[tokio::test]
async fn memory_cache_capacity_eviction_is_deterministic() {
    let cache = MemoryCache::new(None, Some(2));

    cache.set("b", "second", None).await.unwrap();
    cache.set("a", "first", None).await.unwrap();
    cache.set("c", "third", None).await.unwrap();

    assert_eq!(cache.get("a").await.unwrap(), None);
    assert_eq!(cache.get("b").await.unwrap().as_deref(), Some("second"));
    assert_eq!(cache.get("c").await.unwrap().as_deref(), Some("third"));
}

#[tokio::test]
async fn memory_cache_honors_subsecond_ttl() {
    let now = Arc::new(Mutex::new(std::time::Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());

    cache
        .set("short", "value", Some(Duration::from_millis(500)))
        .await
        .unwrap();
    *now.lock() += Duration::from_millis(250);
    assert_eq!(cache.get("short").await.unwrap().as_deref(), Some("value"));

    *now.lock() += Duration::from_millis(250);
    assert_eq!(cache.get("short").await.unwrap(), None);
}

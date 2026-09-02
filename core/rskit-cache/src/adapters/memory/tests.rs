use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_errors::ErrorCode;

use crate::CacheStore;

use super::*;

#[tokio::test]
async fn round_trips_non_utf8_bytes() {
    let cache = MemoryCache::default();
    let bytes = [0xff, 0x00, 0xfe, 0x80, b'a'];

    cache.set("binary", &bytes, None).await.unwrap();

    assert_eq!(
        cache.get("binary").await.unwrap().as_deref(),
        Some(bytes.as_slice())
    );
}

#[tokio::test]
async fn set_rejects_unrepresentable_ttl() {
    let cache = MemoryCache::default();

    let err = cache
        .set("too-long", b"value", Some(Duration::MAX))
        .await
        .expect_err("TTL overflow must be rejected");

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("too large"));
}

#[tokio::test]
async fn get_prunes_unrelated_expired_entries() {
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());
    cache
        .set("expired", b"value", Some(Duration::from_secs(1)))
        .await
        .unwrap();
    cache.set("live", b"value", None).await.unwrap();
    *now.lock() += Duration::from_secs(2);

    assert_eq!(
        cache.get("live").await.unwrap().as_deref(),
        Some(b"value".as_slice())
    );
    assert_eq!(cache.entries.lock().len(), 1);
}

#[tokio::test]
async fn delete_prunes_unrelated_expired_entries() {
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock = Arc::clone(&now);
    let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());
    cache
        .set("expired", b"value", Some(Duration::from_secs(1)))
        .await
        .unwrap();
    cache.set("live", b"value", None).await.unwrap();
    *now.lock() += Duration::from_secs(2);

    assert!(!cache.delete("missing").await.unwrap());
    assert_eq!(cache.entries.lock().len(), 1);
}

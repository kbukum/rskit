use super::*;
use rskit_llm::types;
use rskit_llm::types::Message;
use std::sync::Arc;

#[tokio::test]
async fn test_in_memory_store_save_load() {
    let store = InMemoryStore::new();
    let msgs = vec![types::user("hello"), types::assistant("hi")];
    store.save("s1", &msgs).await.unwrap();

    let loaded = store.load("s1").await.unwrap();
    assert_eq!(loaded.len(), 2);
}

#[tokio::test]
async fn test_in_memory_store_append() {
    let store = InMemoryStore::new();
    store.save("s1", &[types::user("hello")]).await.unwrap();
    store.append("s1", &[types::assistant("hi")]).await.unwrap();

    let loaded = store.load("s1").await.unwrap();
    assert_eq!(loaded.len(), 2);
}

#[tokio::test]
async fn test_in_memory_store_clear() {
    let store = InMemoryStore::new();
    store.save("s1", &[types::user("hello")]).await.unwrap();
    store.clear("s1").await.unwrap();

    let loaded = store.load("s1").await.unwrap();
    assert!(loaded.is_empty());
}

#[tokio::test]
async fn test_in_memory_store_load_missing() {
    let store = InMemoryStore::new();
    let loaded = store.load("nonexistent").await.unwrap();
    assert!(loaded.is_empty());
}

#[tokio::test]
async fn test_sliding_window_trims() {
    let inner = Arc::new(InMemoryStore::new());
    let sw = SlidingWindowMemory::new(inner.clone(), 2).unwrap();

    let msgs = vec![
        types::user("a"),
        types::assistant("b"),
        types::user("c"),
        types::assistant("d"),
    ];
    sw.save("s1", &msgs).await.unwrap();

    let loaded = sw.load("s1").await.unwrap();
    assert_eq!(loaded.len(), 2);
}

#[tokio::test]
async fn test_sliding_window_preserves_system() {
    let inner = Arc::new(InMemoryStore::new());
    let sw = SlidingWindowMemory::new(inner.clone(), 2).unwrap();

    let msgs = vec![
        types::system("sys"),
        types::user("a"),
        types::assistant("b"),
        types::user("c"),
        types::assistant("d"),
    ];
    sw.save("s1", &msgs).await.unwrap();

    let loaded = sw.load("s1").await.unwrap();
    // system + last 2
    assert_eq!(loaded.len(), 3);
    assert!(matches!(&loaded[0], Message::System(_)));
}

#[tokio::test]
async fn test_sliding_window_append_trims() {
    let inner = Arc::new(InMemoryStore::new());
    let sw = SlidingWindowMemory::new(inner.clone(), 3).unwrap();

    sw.save("s1", &[types::user("a"), types::assistant("b")])
        .await
        .unwrap();
    sw.append("s1", &[types::user("c"), types::assistant("d")])
        .await
        .unwrap();

    let loaded = sw.load("s1").await.unwrap();
    assert_eq!(loaded.len(), 3);
}

#[test]
fn test_sliding_window_zero_max_messages() {
    let inner = Arc::new(InMemoryStore::new());
    let result = SlidingWindowMemory::new(inner, 0);
    assert!(result.is_err());
}

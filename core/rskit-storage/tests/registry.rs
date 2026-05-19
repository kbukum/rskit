use rskit_storage::{FileSource, LocalStoreConfig, StorageConfig, StorageRegistry, register_local};

#[test]
fn storage_registry_empty_until_explicit_registration() {
    let registry = StorageRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.contains("local"));
}

#[tokio::test]
async fn explicit_local_registration_builds_store_from_config() {
    let mut registry = StorageRegistry::new();
    register_local(&mut registry).unwrap();

    let root_dir = tempfile::tempdir().unwrap();

    let config = StorageConfig {
        backend: "local".into(),
        local: LocalStoreConfig {
            root_dir: root_dir.path().to_path_buf(),
            auto_create: true,
        },
    };

    let store = registry.build(&config).await.unwrap();
    store
        .upload(
            &FileSource::Bytes(bytes::Bytes::from_static(b"hello")),
            "hello.txt",
            Some("text/plain"),
            None,
        )
        .await
        .unwrap();

    assert!(store.exists("hello.txt").await.unwrap());
    let downloaded = store.download("hello.txt").await.unwrap();
    assert_eq!(downloaded.read_all().await.unwrap().as_ref(), b"hello");
}

#[tokio::test]
async fn unregistered_storage_backend_errors() {
    let registry = StorageRegistry::new();
    let config = StorageConfig {
        backend: "s3".into(),
        local: LocalStoreConfig::default(),
    };

    let err = registry.build(&config).await.err().unwrap();
    assert!(err.to_string().contains("not registered"));
}

use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::{
    FileSource, FileStore, LocalStoreConfig, StorageConfig, StorageFactory, StorageRegistry,
    TempDir, register_local,
};

struct FailingFactory;

#[async_trait::async_trait]
impl StorageFactory for FailingFactory {
    async fn create(&self, _config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        Err(AppError::new(
            ErrorCode::Internal,
            "failing test factory should not build",
        ))
    }
}

#[test]
fn storage_registry_empty_until_explicit_registration() {
    let registry = StorageRegistry::new();
    let config = StorageConfig::default();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.contains("local"));
    assert_eq!(config.backend, "local");
    assert!(config.local.auto_create);
}

#[test]
fn storage_registry_rejects_empty_and_duplicate_backend_names() {
    let mut registry = StorageRegistry::new();

    let empty_error = registry
        .register(" \t ", Arc::new(FailingFactory))
        .unwrap_err();
    assert_eq!(empty_error.code(), ErrorCode::InvalidInput);

    register_local(&mut registry).unwrap();
    let duplicate_error = register_local(&mut registry).unwrap_err();
    assert_eq!(duplicate_error.code(), ErrorCode::AlreadyExists);
    assert_eq!(registry.len(), 1);
}

#[test]
fn storage_registry_trims_registered_backend_name() {
    let mut registry = StorageRegistry::new();

    registry
        .register(" custom ", Arc::new(FailingFactory))
        .unwrap();

    assert!(registry.contains("custom"));
    assert!(!registry.contains(" custom "));
}

#[tokio::test]
async fn explicit_local_registration_builds_store_from_config() {
    let mut registry = StorageRegistry::new();
    register_local(&mut registry).unwrap();

    let root_dir = TempDir::new().unwrap();

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
async fn storage_registry_build_trims_configured_backend_name() {
    let mut registry = StorageRegistry::new();
    register_local(&mut registry).unwrap();

    let root_dir = TempDir::new().unwrap();

    let config = StorageConfig {
        backend: " local ".into(),
        local: LocalStoreConfig {
            root_dir: root_dir.path().to_path_buf(),
            auto_create: true,
        },
    };

    let store = registry.build(&config).await.unwrap();

    assert!(!store.exists("anything.txt").await.unwrap());
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

#[tokio::test]
async fn blank_configured_backend_errors_before_factory_lookup() {
    let mut registry = StorageRegistry::new();
    register_local(&mut registry).unwrap();
    let config = StorageConfig {
        backend: "  ".into(),
        local: LocalStoreConfig::default(),
    };

    let error = match registry.build(&config).await {
        Ok(_) => panic!("blank backend should be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert!(
        error
            .to_string()
            .contains("storage backend name is required")
    );
}

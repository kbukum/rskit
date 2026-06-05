//! Local store unit tests.

use std::time::Duration;

use bytes::Bytes;

use crate::FileSource;
use crate::store::FileStore;

use super::{LocalStore, LocalStoreConfig};

#[test]
fn default_root_dir_is_isolated_per_config() {
    let first = LocalStoreConfig::default();
    let second = LocalStoreConfig::default();

    assert!(first.auto_create);
    assert!(second.auto_create);
    assert_ne!(first.root_dir, second.root_dir);
    assert!(first.root_dir.starts_with(std::env::temp_dir()));
    assert!(second.root_dir.starts_with(std::env::temp_dir()));
}

#[tokio::test]
async fn traversal_keys_are_rejected_for_local_store_operations() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new(LocalStoreConfig {
        root_dir: root.path().to_path_buf(),
        auto_create: true,
    })
    .unwrap();
    let source = FileSource::from_bytes(Bytes::from_static(b"secret"));

    assert!(
        store
            .upload(&source, "../escape.txt", None, None)
            .await
            .is_err()
    );
    assert!(store.download("../escape.txt").await.is_err());
    assert!(store.copy("../escape.txt", "copy.txt").await.is_err());
    assert!(store.rename("../escape.txt", "renamed.txt").await.is_err());
    assert!(store.copy("missing.txt", "../copy.txt").await.is_err());
    assert!(store.rename("missing.txt", "../renamed.txt").await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn local_store_rejects_intermediate_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
    let store = LocalStore::new(LocalStoreConfig {
        root_dir: root.path().to_path_buf(),
        auto_create: true,
    })
    .unwrap();
    let source = FileSource::from_bytes(Bytes::from_static(b"secret"));

    assert!(
        store
            .upload(&source, "linked/escape.txt", None, None)
            .await
            .is_err()
    );
    assert!(!outside.path().join("escape.txt").exists());
    assert!(
        store
            .upload(&source, "linked/nested/escape.txt", None, None)
            .await
            .is_err()
    );
    assert!(!outside.path().join("nested").exists());

    std::fs::write(outside.path().join("existing.txt"), b"outside").unwrap();
    assert!(store.download("linked/existing.txt").await.is_err());
    assert!(store.head("linked/existing.txt").await.is_err());
    assert!(store.copy("linked/existing.txt", "copy.txt").await.is_err());
    assert!(store.exists("linked/existing.txt").await.is_err());
    assert!(store.delete("linked/existing.txt").await.is_err());
    assert!(
        store
            .presigned_url("linked/existing.txt", Duration::from_secs(60))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(outside.path().join("existing.txt")).unwrap(),
        b"outside"
    );
    assert!(store.list("linked", None).await.is_err());
}

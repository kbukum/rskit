//! Local store unit tests.

use std::time::Duration;

use bytes::Bytes;
use rskit_errors::ErrorCode;

use crate::FileSource;
use crate::store::{FileStore, UploadOptions};

use super::path::{
    canonicalize_confined, ensure_target_parent_confined, file_not_found_error,
    file_not_found_error_with_cause, normalize_local_key, storage_temp_path,
};
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

#[test]
fn local_store_rejects_non_directory_root() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("root-file");
    std::fs::write(&file, b"not a directory").unwrap();

    let err = LocalStore::new(LocalStoreConfig {
        root_dir: file,
        auto_create: false,
    })
    .err()
    .unwrap();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[cfg(unix)]
#[test]
fn local_store_rejects_symlink_root() {
    let root = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let link = root.path().join("linked-root");
    std::os::unix::fs::symlink(target.path(), &link).unwrap();

    for auto_create in [false, true] {
        let err = LocalStore::new(LocalStoreConfig {
            root_dir: link.clone(),
            auto_create,
        })
        .err()
        .unwrap();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
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
            .upload(&source, "../escape.txt", UploadOptions::new())
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
            .upload(&source, "linked/escape.txt", UploadOptions::new())
            .await
            .is_err()
    );
    assert!(!outside.path().join("escape.txt").exists());
    assert!(
        store
            .upload(&source, "linked/nested/escape.txt", UploadOptions::new())
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

#[test]
fn local_path_helpers_normalize_keys_and_build_not_found_errors() {
    assert_eq!(
        normalize_local_key("/nested/file.txt").unwrap(),
        "nested/file.txt"
    );
    assert!(normalize_local_key("../escape.txt").is_err());

    let temp = storage_temp_path(std::path::Path::new("dir/file.txt"));
    assert_eq!(temp.parent(), Some(std::path::Path::new("dir")));
    let name = temp.file_name().unwrap().to_string_lossy();
    assert!(name.contains("file"));
    assert!(name.ends_with(".rskit-tmp"));

    let cause = file_not_found_error("missing.txt");
    let err = file_not_found_error_with_cause("missing.txt", cause);
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert!(err.to_string().contains("missing.txt"));
}

#[tokio::test]
async fn canonicalize_confined_rejects_paths_outside_root_and_missing_paths() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("outside.txt");
    std::fs::write(&outside_file, b"outside").unwrap();

    let err = canonicalize_confined(root.path(), &outside_file)
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("escapes configured root"));

    let err = canonicalize_confined(root.path(), &root.path().join("missing.txt"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.to_string().contains("canonicalize"));
}

#[tokio::test]
async fn ensure_target_parent_confined_rejects_invalid_parent_components() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    let err = ensure_target_parent_confined(root.path(), &outside.path().join("file.txt"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("not under configured root"));

    std::fs::write(root.path().join("file-parent"), b"not a dir").unwrap();
    let err =
        ensure_target_parent_confined(root.path(), &root.path().join("file-parent/child.txt"))
            .await
            .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("not a directory"));
}

#[tokio::test]
async fn local_store_lists_file_prefix_as_empty_and_rejects_directory_presign() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new(LocalStoreConfig {
        root_dir: root.path().to_path_buf(),
        auto_create: true,
    })
    .unwrap();
    let source = FileSource::from_bytes(Bytes::from_static(b"content"));
    store
        .upload(&source, "dir/file.txt", UploadOptions::new())
        .await
        .unwrap();

    assert!(store.list("dir/file.txt", None).await.unwrap().is_empty());

    let err = store
        .presigned_url("dir", Duration::from_secs(60))
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
}

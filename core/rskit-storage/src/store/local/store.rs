//! Local [`FileStore`] implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::{async_io, sync_io};

use crate::FileSource;

use super::super::{FileStore, ProgressCallback, StoredFile, prefixed_key};
use super::config::LocalStoreConfig;
use super::path::{
    canonicalize_confined, ensure_target_parent_confined, file_not_found_error,
    file_not_found_error_with_cause, normalize_local_key, replace_with_temp, storage_temp_path,
};

/// Local filesystem storage backend with root-confined keys and write targets.
pub struct LocalStore {
    config: LocalStoreConfig,
}

impl LocalStore {
    /// Create a new local store.
    pub fn new(config: LocalStoreConfig) -> AppResult<Self> {
        if config.auto_create {
            sync_io::dir::create_all(&config.root_dir)?;
        }
        Self::validate_root_dir(&config.root_dir)?;
        Ok(Self { config })
    }

    fn resolve_path(&self, key: &str) -> AppResult<PathBuf> {
        let key = normalize_local_key(key)?;
        self.resolve_normalized_path(&key)
    }

    fn validate_root_dir(root: &Path) -> AppResult<()> {
        let metadata = std::fs::symlink_metadata(root).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("store root {} does not exist", root.display()),
                )
            } else {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to inspect store root '{}': {error}", root.display()),
                )
                .with_cause(error)
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("store root {} must not be a symlink", root.display()),
            ));
        }
        if !metadata.is_dir() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("store root {} must be a directory", root.display()),
            ));
        }
        Ok(())
    }

    fn inspect_existing_file_error(key: &str, path: &Path, error: std::io::Error) -> AppError {
        if error.kind() == std::io::ErrorKind::NotFound {
            file_not_found_error(key).with_cause(error)
        } else {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to inspect storage file '{}': {error}",
                    path.display()
                ),
            )
            .with_cause(error)
        }
    }

    fn resolve_normalized_path(&self, key: &str) -> AppResult<PathBuf> {
        rskit_fs::safe_join(&self.config.root_dir, Path::new(key))
            .map_err(|error| AppError::new(ErrorCode::InvalidInput, error.to_string()))
    }

    async fn confined_existing_file_path(&self, key: &str) -> AppResult<PathBuf> {
        let path = self.resolve_path(key)?;
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|error| Self::inspect_existing_file_error(key, &path, error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(file_not_found_error(key));
        }
        let canonical = canonicalize_confined(&self.config.root_dir, &path).await?;
        Ok(canonical)
    }

    async fn confined_existing_dir_path(&self, key: &str) -> AppResult<Option<PathBuf>> {
        let path = self.resolve_path(key)?;
        let metadata = match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("failed to inspect directory '{}': {error}", path.display()),
                )
                .with_cause(error));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "storage directory '{}' must not be a symlink",
                    path.display()
                ),
            ));
        }
        if !metadata.is_dir() {
            return Ok(None);
        }
        canonicalize_confined(&self.config.root_dir, &path)
            .await
            .map(Some)
    }

    async fn confined_presigned_path(&self, key: &str) -> AppResult<PathBuf> {
        let path = self.resolve_path(key)?;
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(file_not_found_error(key));
                }
                canonicalize_confined(&self.config.root_dir, &path).await
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or_else(|| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "storage presigned URL target '{}' has no parent directory",
                            path.display()
                        ),
                    )
                })?;
                ensure_target_parent_confined(&self.config.root_dir, &path).await?;
                let parent = canonicalize_confined(&self.config.root_dir, parent).await?;
                let filename = path.file_name().ok_or_else(|| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "storage presigned URL target '{}' has no filename",
                            path.display()
                        ),
                    )
                })?;
                Ok(parent.join(filename))
            }
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to inspect presigned URL target '{}': {error}",
                    path.display()
                ),
            )
            .with_cause(error)),
        }
    }

    async fn stream_to_target(
        &self,
        reader: &mut (dyn tokio::io::AsyncRead + Unpin + Send),
        target: &Path,
    ) -> AppResult<u64> {
        ensure_target_parent_confined(&self.config.root_dir, target).await?;
        let temp_path = storage_temp_path(target);
        let mut temp = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create temporary file '{}': {error}",
                        temp_path.display()
                    ),
                )
                .with_cause(error)
            })?;
        let result = async {
            let size = tokio::io::copy(reader, &mut temp).await.map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to stream {}: {error}", target.display()),
                )
                .with_cause(error)
            })?;
            temp.sync_data().await.map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to sync temporary file '{}': {error}",
                        temp_path.display()
                    ),
                )
                .with_cause(error)
            })?;
            drop(temp);
            replace_with_temp(&temp_path, target).await?;
            Ok(size)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
        result
    }
}

#[async_trait::async_trait]
impl FileStore for LocalStore {
    async fn upload(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> AppResult<StoredFile> {
        let key = normalize_local_key(key)?;
        let target = self.resolve_normalized_path(&key)?;

        let mut reader = source.reader().await?;
        let size = self.stream_to_target(&mut reader, &target).await?;

        Ok(StoredFile::new(key, size, content_type).with_metadata(metadata.unwrap_or_default()))
    }

    async fn upload_with_progress(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        _on_progress: ProgressCallback,
    ) -> AppResult<StoredFile> {
        self.upload(source, key, content_type, None).await
    }

    async fn download(&self, key: &str) -> AppResult<FileSource> {
        let path = self.confined_existing_file_path(key).await?;
        Ok(FileSource::Path(path))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let path = self.confined_existing_file_path(key).await?;
        if async_io::file::remove_if_exists(&path).await? {
            Ok(())
        } else {
            Err(file_not_found_error(key))
        }
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        match self.confined_existing_file_path(key).await {
            Ok(_) => Ok(true),
            Err(error) if error.code() == ErrorCode::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let key = normalize_local_key(key)?;
        let path = self.confined_existing_file_path(&key).await?;
        let meta = async_io::file::metadata(&path)
            .await
            .map_err(|error| file_not_found_error_with_cause(&key, error))?;
        if !meta.is_file || meta.is_symlink {
            return Err(file_not_found_error(&key));
        }

        let mime = crate::detect_mime(&FileSource::Path(path)).await?;

        let stored_at = meta
            .modified
            .map(chrono::DateTime::<chrono::Utc>::from)
            .unwrap_or_else(chrono::Utc::now);

        Ok(StoredFile::new(key, meta.len, Some(&mime)).with_stored_at(stored_at))
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let Some(dir) = self.confined_existing_dir_path(prefix).await? else {
            return Ok(Vec::new());
        };
        let mut results = Vec::new();

        let mut entries = tokio::fs::read_dir(&dir).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read directory '{}': {error}", dir.display()),
            )
            .with_cause(error)
        })?;
        while let Some(entry) = entries.next_entry().await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read directory entry: {error}"),
            )
            .with_cause(error)
        })? {
            if let Some(max) = limit
                && results.len() >= max
            {
                break;
            }

            let path = entry.path();
            let file_type = entry.file_type().await.map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to inspect directory entry '{}': {error}",
                        path.display()
                    ),
                )
                .with_cause(error)
            })?;
            if file_type.is_file() {
                let meta = async_io::file::metadata(&path).await?;
                let filename = entry.file_name();
                let key = prefixed_key(Some(prefix), filename.to_string_lossy().as_ref());
                let stored_at = meta
                    .modified
                    .map(chrono::DateTime::<chrono::Utc>::from)
                    .unwrap_or_else(chrono::Utc::now);
                results.push(StoredFile::new(key, meta.len, None).with_stored_at(stored_at));
            }
        }

        Ok(results)
    }

    async fn presigned_url(&self, key: &str, _expires_in: Duration) -> AppResult<String> {
        let path = self.confined_presigned_path(key).await?;
        Ok(format!("file://{}", path.display()))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.confined_existing_file_path(from_key).await?;
        let to = self.resolve_path(to_key)?;

        let mut reader = tokio::fs::File::open(&from).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to open source file '{}': {error}", from.display()),
            )
            .with_cause(error)
        })?;
        self.stream_to_target(&mut reader, &to).await?;

        self.head(to_key).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.confined_existing_file_path(from_key).await?;
        let to = self.resolve_path(to_key)?;
        ensure_target_parent_confined(&self.config.root_dir, &to).await?;

        async_io::file::rename(&from, &to)
            .await
            .map_err(|error| error.context(format!("failed to rename {from_key} to {to_key}")))?;

        self.head(to_key).await
    }
}

#[cfg(test)]
mod focused_tests {
    use super::*;
    use tokio::io::{AsyncRead, ReadBuf};

    struct FailingReader;

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("read failed")))
        }
    }

    fn store_at(root: &Path) -> LocalStore {
        LocalStore::new(LocalStoreConfig {
            root_dir: root.to_path_buf(),
            auto_create: true,
        })
        .unwrap()
    }

    #[test]
    fn inspect_existing_file_error_maps_not_found_and_internal_causes() {
        let not_found = LocalStore::inspect_existing_file_error(
            "missing",
            Path::new("missing"),
            std::io::Error::new(std::io::ErrorKind::NotFound, "gone"),
        );
        assert_eq!(not_found.code(), ErrorCode::NotFound);

        let denied = LocalStore::inspect_existing_file_error(
            "blocked",
            Path::new("blocked"),
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        assert_eq!(denied.code(), ErrorCode::Internal);
        assert!(
            denied
                .to_string()
                .contains("failed to inspect storage file")
        );
    }

    #[test]
    fn new_rejects_missing_root_when_auto_create_is_disabled() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        let err = match LocalStore::new(LocalStoreConfig {
            root_dir: missing,
            auto_create: false,
        }) {
            Ok(_) => panic!("missing root should be rejected"),
            Err(err) => err,
        };

        assert_eq!(err.code(), ErrorCode::NotFound);
        assert!(err.to_string().contains("store root"));
    }

    #[tokio::test]
    async fn private_path_flows_cover_directory_presign_and_stream_cleanup_errors() {
        let root = tempfile::tempdir().unwrap();
        let store = store_at(root.path());
        std::fs::create_dir(root.path().join("dir")).unwrap();

        assert!(
            store
                .confined_existing_dir_path("missing")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .confined_existing_dir_path("dir")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            store
                .confined_presigned_path("dir")
                .await
                .unwrap_err()
                .code(),
            ErrorCode::NotFound
        );

        let mut reader = FailingReader;
        let target = root.path().join("failed.bin");
        let err = store
            .stream_to_target(&mut reader, &target)
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.to_string().contains("failed to stream"));
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn upload_copy_and_rename_normalize_keys_and_preserve_metadata_paths() {
        let root = tempfile::tempdir().unwrap();
        let store = store_at(root.path());
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"data"));
        let metadata = [("owner".to_string(), "dataset".to_string())]
            .into_iter()
            .collect();

        let stored = store
            .upload(
                &source,
                "/nested/item.txt",
                Some("text/plain"),
                Some(metadata),
            )
            .await
            .unwrap();
        assert_eq!(stored.key, "nested/item.txt");
        assert_eq!(
            stored.metadata.get("owner").map(String::as_str),
            Some("dataset")
        );

        let copied = store.copy("nested/item.txt", "copy.txt").await.unwrap();
        assert_eq!(copied.key, "copy.txt");
        let renamed = store.rename("copy.txt", "renamed.txt").await.unwrap();
        assert_eq!(renamed.key, "renamed.txt");
        assert!(store.exists("renamed.txt").await.unwrap());
    }

    #[tokio::test]
    async fn download_head_list_presign_progress_and_delete_cover_local_store_contract() {
        let root = tempfile::tempdir().unwrap();
        let store = store_at(root.path());
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"hello"));

        let uploaded = store
            .upload_with_progress(
                &source,
                "dir/hello.txt",
                Some("text/plain"),
                std::sync::Arc::new(|_| {}),
            )
            .await
            .unwrap();
        assert_eq!(uploaded.size, 5);

        let head = store.head("dir/hello.txt").await.unwrap();
        assert_eq!(head.key, "dir/hello.txt");
        assert_eq!(head.size, 5);
        assert!(head.content_type.contains("text"));

        let downloaded = store.download("dir/hello.txt").await.unwrap();
        assert!(matches!(downloaded, FileSource::Path(_)));
        let listed = store.list("dir", Some(1)).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "dir/hello.txt");
        assert!(store.list("missing", None).await.unwrap().is_empty());

        let existing_url = store
            .presigned_url("dir/hello.txt", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(existing_url.starts_with("file://"));
        let missing_url = store
            .presigned_url("dir/new.txt", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(missing_url.ends_with("dir/new.txt"));

        store.delete("dir/hello.txt").await.unwrap();
        assert!(!store.exists("dir/hello.txt").await.unwrap());
        assert_eq!(
            store.delete("dir/hello.txt").await.unwrap_err().code(),
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn file_operations_reject_directory_keys_as_missing_files() {
        let root = tempfile::tempdir().unwrap();
        let store = store_at(root.path());
        std::fs::create_dir_all(root.path().join("dir")).unwrap();

        assert_eq!(
            store.download("dir").await.unwrap_err().code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            store.head("dir").await.unwrap_err().code(),
            ErrorCode::NotFound
        );
        assert!(!store.exists("dir").await.unwrap());
    }
}

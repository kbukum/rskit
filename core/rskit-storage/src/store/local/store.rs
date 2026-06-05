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

    fn resolve_normalized_path(&self, key: &str) -> AppResult<PathBuf> {
        rskit_fs::safe_join(&self.config.root_dir, Path::new(key))
            .map_err(|error| AppError::new(ErrorCode::InvalidInput, error.to_string()))
    }

    async fn confined_existing_file_path(&self, key: &str) -> AppResult<PathBuf> {
        let path = self.resolve_path(key)?;
        let metadata = async_io::file::metadata(&path)
            .await
            .map_err(|error| file_not_found_error_with_cause(key, error))?;
        if !metadata.is_file || metadata.is_symlink {
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

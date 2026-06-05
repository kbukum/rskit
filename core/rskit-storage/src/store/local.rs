//! Local filesystem storage backend.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::{async_io, sync_io};
use serde::{Deserialize, Serialize};

use crate::FileSource;

use super::{FileStore, ProgressCallback, StoredFile, prefixed_key};

static NEXT_DEFAULT_ROOT_ID: AtomicU64 = AtomicU64::new(0);

/// Configuration for the local file store.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalStoreConfig {
    /// Root directory for stored files.
    pub root_dir: PathBuf,
    /// Whether to auto-create the root directory if it doesn't exist.
    pub auto_create: bool,
}

impl Default for LocalStoreConfig {
    fn default() -> Self {
        Self {
            root_dir: default_local_root_dir(),
            auto_create: true,
        }
    }
}

fn default_local_root_dir() -> PathBuf {
    let sequence = NEXT_DEFAULT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "rskit-storage-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

fn normalize_local_key(key: &str) -> AppResult<String> {
    let key = prefixed_key(None, key);
    rskit_fs::validate_relative_path(Path::new(&key)).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("storage key must stay within the configured root ({key}): {error}"),
        )
    })?;
    Ok(key)
}

/// Local filesystem storage backend.
pub struct LocalStore {
    config: LocalStoreConfig,
}

impl LocalStore {
    /// Create a new local store.
    pub fn new(config: LocalStoreConfig) -> AppResult<Self> {
        if config.auto_create {
            sync_io::dir::create_all(&config.root_dir)?;
        } else if !sync_io::dir::exists(&config.root_dir)? {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("store root {} does not exist", config.root_dir.display()),
            ));
        }
        Ok(Self { config })
    }

    fn resolve_path(&self, key: &str) -> AppResult<PathBuf> {
        let key = normalize_local_key(key)?;
        self.resolve_normalized_path(&key)
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

    async fn ensure_target_parent_confined(&self, target: &Path) -> AppResult<()> {
        async_io::file::create_parent_dir(target).await?;
        let parent = target.parent().ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "storage target '{}' has no parent directory",
                    target.display()
                ),
            )
        })?;
        canonicalize_confined(&self.config.root_dir, parent).await?;
        Ok(())
    }

    async fn stream_to_target(
        &self,
        reader: &mut (dyn tokio::io::AsyncRead + Unpin + Send),
        target: &Path,
    ) -> AppResult<u64> {
        self.ensure_target_parent_confined(target).await?;
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
        // For local store, just delegate to regular upload
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
        self.ensure_target_parent_confined(&to).await?;

        async_io::file::rename(&from, &to).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to rename {from_key} to {to_key}: {e}"),
            )
        })?;

        self.head(to_key).await
    }
}

async fn canonicalize_confined(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = tokio::fs::canonicalize(root).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize storage root '{}': {error}",
                root.display()
            ),
        )
        .with_cause(error)
    })?;
    let canonical = tokio::fs::canonicalize(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize storage path '{}': {error}",
                path.display()
            ),
        )
        .with_cause(error)
    })?;
    if canonical.starts_with(&root) {
        Ok(canonical)
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "storage path '{}' escapes configured root '{}'",
                canonical.display(),
                root.display()
            ),
        ))
    }
}

fn storage_temp_path(target: &Path) -> PathBuf {
    let filename = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    rskit_fs::sibling_temp_path(target, filename, ".rskit-tmp")
}

async fn replace_with_temp(temp_path: &Path, target: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        let _ = async_io::file::remove_if_exists(target).await?;
    }
    tokio::fs::rename(temp_path, target).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to replace '{}' with '{}': {error}",
                target.display(),
                temp_path.display()
            ),
        )
        .with_cause(error)
    })
}

fn file_not_found_error(key: &str) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("file not found: {key}"))
}

fn file_not_found_error_with_cause(key: &str, cause: AppError) -> AppError {
    file_not_found_error(key).with_cause(cause)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"secret"));

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
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"secret"));

        assert!(
            store
                .upload(&source, "linked/escape.txt", None, None)
                .await
                .is_err()
        );
        assert!(!outside.path().join("escape.txt").exists());

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
}

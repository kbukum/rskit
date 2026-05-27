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
        let mut file = async_io::file::create(&target).await?;
        let size = tokio::io::copy(&mut reader, &mut file).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to stream {}: {e}", target.display()),
            )
        })?;

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
        let path = self.resolve_path(key)?;
        if !async_io::file::exists(&path).await? {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("file not found: {key}"),
            ));
        }
        Ok(FileSource::Path(path))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let path = self.resolve_path(key)?;
        async_io::file::remove(&path)
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("failed to delete {key}: {e}")))
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        let path = self.resolve_path(key)?;
        async_io::file::exists(&path).await
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let key = normalize_local_key(key)?;
        let path = self.resolve_normalized_path(&key)?;
        let meta = async_io::file::metadata(&path).await.map_err(|e| {
            AppError::new(ErrorCode::NotFound, format!("file not found {key}: {e}"))
        })?;

        let mime = crate::detect_mime(&FileSource::Path(path)).await?;

        let stored_at = meta
            .modified
            .map(chrono::DateTime::<chrono::Utc>::from)
            .unwrap_or_else(chrono::Utc::now);

        Ok(StoredFile::new(key, meta.len, Some(&mime)).with_stored_at(stored_at))
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let dir = self.resolve_path(prefix)?;
        let mut results = Vec::new();

        if !async_io::dir::exists(&dir).await? {
            return Ok(results);
        }

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
        let path = self.resolve_path(key)?;
        Ok(format!("file://{}", path.display()))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.resolve_path(from_key)?;
        let to = self.resolve_path(to_key)?;

        async_io::file::copy(&from, &to).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to copy {from_key} to {to_key}: {e}"),
            )
        })?;

        self.head(to_key).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.resolve_path(from_key)?;
        let to = self.resolve_path(to_key)?;

        async_io::file::rename(&from, &to).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to rename {from_key} to {to_key}: {e}"),
            )
        })?;

        self.head(to_key).await
    }
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
}

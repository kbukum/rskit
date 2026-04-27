//! Local filesystem storage backend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Deserialize;

use crate::FileSource;

use super::{FileStore, ProgressCallback, StoredFile};

/// Configuration for the local file store.
#[derive(Debug, Clone, Deserialize)]
pub struct LocalStoreConfig {
    /// Root directory for stored files.
    pub root_dir: PathBuf,
    /// Whether to auto-create the root directory if it doesn't exist.
    pub auto_create: bool,
}

/// Local filesystem storage backend.
pub struct LocalStore {
    config: LocalStoreConfig,
}

impl LocalStore {
    /// Create a new local store.
    pub fn new(config: LocalStoreConfig) -> AppResult<Self> {
        if config.auto_create {
            std::fs::create_dir_all(&config.root_dir).map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create store root {}: {e}",
                        config.root_dir.display()
                    ),
                )
            })?;
        } else if !config.root_dir.exists() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("store root {} does not exist", config.root_dir.display()),
            ));
        }
        Ok(Self { config })
    }

    fn resolve_path(&self, key: &str) -> PathBuf {
        self.config.root_dir.join(key)
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
        let target = self.resolve_path(key);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to create dirs: {e}"))
            })?;
        }

        let data = source.read_all().await?;
        let size = data.len() as u64;
        tokio::fs::write(&target, &data).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write {}: {e}", target.display()),
            )
        })?;

        let ct = content_type.unwrap_or("application/octet-stream");
        Ok(StoredFile {
            key: key.to_string(),
            size,
            content_type: ct.to_string(),
            stored_at: Utc::now(),
            metadata: metadata.unwrap_or_default(),
        })
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
        let path = self.resolve_path(key);
        if !path.exists() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("file not found: {key}"),
            ));
        }
        Ok(FileSource::Path(path))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let path = self.resolve_path(key);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("failed to delete {key}: {e}")))
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        let path = self.resolve_path(key);
        Ok(path.exists())
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let path = self.resolve_path(key);
        let meta = tokio::fs::metadata(&path).await.map_err(|e| {
            AppError::new(ErrorCode::NotFound, format!("file not found {key}: {e}"))
        })?;

        let mime = crate::detect_mime(&FileSource::Path(path)).await?;

        Ok(StoredFile {
            key: key.to_string(),
            size: meta.len(),
            content_type: mime,
            stored_at: meta
                .modified()
                .map(chrono::DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now()),
            metadata: HashMap::new(),
        })
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let dir = self.resolve_path(prefix);
        let mut results = Vec::new();

        if !dir.exists() {
            return Ok(results);
        }

        let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to list {}: {e}", dir.display()),
            )
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read dir entry: {e}"),
            )
        })? {
            if let Some(max) = limit
                && results.len() >= max
            {
                break;
            }

            let meta = entry.metadata().await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to read metadata: {e}"))
            })?;

            if meta.is_file() {
                let key = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
                results.push(StoredFile {
                    key,
                    size: meta.len(),
                    content_type: "application/octet-stream".to_string(),
                    stored_at: meta
                        .modified()
                        .map(chrono::DateTime::<Utc>::from)
                        .unwrap_or_else(|_| Utc::now()),
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(results)
    }

    async fn presigned_url(&self, key: &str, _expires_in: Duration) -> AppResult<String> {
        let path = self.resolve_path(key);
        Ok(format!("file://{}", path.display()))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.resolve_path(from_key);
        let to = self.resolve_path(to_key);

        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to create dirs: {e}"))
            })?;
        }

        tokio::fs::copy(&from, &to).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to copy {from_key} to {to_key}: {e}"),
            )
        })?;

        self.head(to_key).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let from = self.resolve_path(from_key);
        let to = self.resolve_path(to_key);

        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to create dirs: {e}"))
            })?;
        }

        tokio::fs::rename(&from, &to).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to rename {from_key} to {to_key}: {e}"),
            )
        })?;

        self.head(to_key).await
    }
}

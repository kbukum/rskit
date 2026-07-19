use std::time::Duration;

use rskit_errors::AppResult;

use crate::FileSource;

use super::{StoredFile, UploadOptions};

/// Trait for file storage backends.
#[async_trait::async_trait]
pub trait FileStore: Send + Sync {
    /// Upload a file to the store.
    ///
    /// `options` carries the content type, user metadata, and an optional
    /// progress callback; use [`UploadOptions::new`] for store defaults.
    async fn upload(
        &self,
        source: &FileSource,
        key: &str,
        options: UploadOptions,
    ) -> AppResult<StoredFile>;

    /// Download a file from the store.
    async fn download(&self, key: &str) -> AppResult<FileSource>;

    /// Delete a file from the store.
    async fn delete(&self, key: &str) -> AppResult<()>;

    /// Check if a file exists in the store.
    async fn exists(&self, key: &str) -> AppResult<bool>;

    /// Get metadata about a stored file without downloading it.
    async fn head(&self, key: &str) -> AppResult<StoredFile>;

    /// List files with a given prefix.
    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>>;

    /// Generate a presigned URL for temporary access.
    async fn presigned_url(&self, key: &str, expires_in: Duration) -> AppResult<String>;

    /// Copy a file within the store.
    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile>;

    /// Rename (move) a file within the store.
    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile>;
}

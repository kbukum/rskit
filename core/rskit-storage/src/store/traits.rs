use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::AppResult;

use crate::FileSource;

use super::{ProgressCallback, StoredFile};

/// Trait for file storage backends.
#[async_trait::async_trait]
pub trait FileStore: Send + Sync {
    /// Upload a file to the store.
    async fn upload(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> AppResult<StoredFile>;

    /// Upload a file with progress reporting.
    async fn upload_with_progress(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        on_progress: ProgressCallback,
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

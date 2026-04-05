//! File store trait and backends for persistent file storage.

mod local;

#[cfg(feature = "gcs")]
mod gcs;

pub use local::{LocalStore, LocalStoreConfig};

#[cfg(feature = "gcs")]
pub use gcs::{GcsStore, GcsStoreConfig};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

use crate::FileSource;

/// Metadata about a stored file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile {
    /// Storage key (path/identifier).
    pub key: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME content type.
    pub content_type: String,
    /// When the file was stored.
    pub stored_at: DateTime<Utc>,
    /// Arbitrary metadata.
    pub metadata: HashMap<String, String>,
}

/// Upload progress report.
pub struct UploadProgress {
    /// Bytes sent so far.
    pub bytes_sent: u64,
    /// Total bytes to send (if known).
    pub total_bytes: Option<u64>,
    /// Completion percentage (if total is known).
    pub percent: Option<f32>,
}

/// Callback for receiving upload progress updates.
pub type ProgressCallback = Arc<dyn Fn(UploadProgress) + Send + Sync>;

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

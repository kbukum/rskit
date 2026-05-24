//! File store trait and backends for persistent file storage.

mod local;
mod registry;

pub use local::{LocalStore, LocalStoreConfig};
pub use registry::{StorageConfig, StorageFactory, StorageRegistry, register_local};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

use crate::FileSource;

/// Default MIME type used when storage metadata does not provide one.
pub const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

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

impl StoredFile {
    /// Create stored-file metadata with default storage timestamp and empty user metadata.
    #[must_use]
    pub fn new(key: impl Into<String>, size: u64, content_type: Option<&str>) -> Self {
        Self {
            key: key.into(),
            size,
            content_type: content_type_or_default(content_type).to_string(),
            stored_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Attach backend metadata to this stored-file descriptor.
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Override the storage timestamp.
    #[must_use]
    pub fn with_stored_at(mut self, stored_at: DateTime<Utc>) -> Self {
        self.stored_at = stored_at;
        self
    }
}

/// Return the provided content type or the storage default.
#[must_use]
pub fn content_type_or_default(content_type: Option<&str>) -> &str {
    content_type
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_CONTENT_TYPE)
}

/// Join an optional storage prefix and object key using normalized `/` separators.
#[must_use]
pub fn prefixed_key(prefix: Option<&str>, key: &str) -> String {
    let key = key.trim_start_matches('/');
    match prefix.map(str::trim).filter(|prefix| !prefix.is_empty()) {
        Some(prefix) => format!("{}/{}", prefix.trim_matches('/'), key),
        None => key.to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_defaults_for_missing_or_empty_values() {
        assert_eq!(content_type_or_default(None), DEFAULT_CONTENT_TYPE);
        assert_eq!(content_type_or_default(Some("  ")), DEFAULT_CONTENT_TYPE);
        assert_eq!(content_type_or_default(Some("text/plain")), "text/plain");
    }

    #[test]
    fn prefixed_key_normalizes_separator_boundaries() {
        assert_eq!(prefixed_key(None, "/file.txt"), "file.txt");
        assert_eq!(
            prefixed_key(Some("uploads/"), "/file.txt"),
            "uploads/file.txt"
        );
        assert_eq!(
            prefixed_key(Some(" /uploads/ "), "file.txt"),
            "uploads/file.txt"
        );
    }

    #[test]
    fn stored_file_constructor_applies_defaults() {
        let stored = StoredFile::new("key", 42, None);

        assert_eq!(stored.key, "key");
        assert_eq!(stored.size, 42);
        assert_eq!(stored.content_type, DEFAULT_CONTENT_TYPE);
        assert!(stored.metadata.is_empty());
    }
}

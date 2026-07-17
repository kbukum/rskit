use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CONTENT_TYPE)
}

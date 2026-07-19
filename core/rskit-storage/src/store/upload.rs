//! Upload request options and progress reporting for [`FileStore`](super::FileStore) uploads.

use std::collections::HashMap;
use std::sync::Arc;

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

/// Options describing how a file is stored during an upload.
///
/// Groups the content type, user metadata, and optional progress callback so a
/// single `upload(source, key, options)` call replaces a run of positional
/// arguments. Build with [`UploadOptions::new`] and the `with_*` methods, or
/// construct the struct directly.
#[derive(Clone, Default)]
pub struct UploadOptions {
    /// MIME content type; falls back to the store default when absent.
    pub content_type: Option<String>,
    /// User metadata stored alongside the file.
    pub metadata: HashMap<String, String>,
    /// Progress callback invoked as bytes are written, for backends that report progress.
    pub progress: Option<ProgressCallback>,
}

impl UploadOptions {
    /// Create empty upload options that use the store defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the MIME content type for the uploaded file.
    #[must_use]
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Attach user metadata stored alongside the file.
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Attach a progress callback invoked as bytes are written.
    #[must_use]
    pub fn with_progress(mut self, progress: ProgressCallback) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Borrow the content type as a string slice, when set.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

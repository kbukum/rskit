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

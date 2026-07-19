use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{DataPayload, DatasetLimits, Label, MediaType};

/// Capability an item must provide to flow through the generic collection engine.
///
/// The engine stays item-agnostic: it routes and counts items by [`label`](DatasetItem::label)
/// and resumes sources from [`source_offset`](DatasetItem::source_offset),
/// without knowing the concrete item type. Both blob samples ([`DataItem`])
/// and tabular records implement this trait.
pub trait DatasetItem: Send + 'static {
    /// Classification label used to route and count the item. Defaults to [`Label::Real`].
    fn label(&self) -> Label {
        Label::Real
    }

    /// Source-provided resume cursor observed after this item, if any.
    fn source_offset(&self) -> Option<usize> {
        None
    }
}

/// A single data sample flowing through the dataset pipeline.
#[derive(Debug, Clone)]
pub struct DataItem {
    /// Payload bytes or file reference.
    payload: DataPayload,
    /// Dataset label.
    pub label: Label,
    /// Media kind for the sample.
    pub media_type: MediaType,
    /// Logical source name.
    pub source_name: String,
    /// File extension including the leading dot.
    pub extension: String,
    /// String metadata attached to the sample.
    pub metadata: HashMap<String, String>,
    /// Source-reported resume cursor after this item, if available.
    source_offset: Option<usize>,
}

impl DataItem {
    /// Create an item from bounded in-memory bytes.
    pub fn new(
        bytes: Vec<u8>,
        label: Label,
        media_type: MediaType,
        source_name: impl Into<String>,
    ) -> AppResult<Self> {
        Self::new_bytes(bytes, label, media_type, source_name)
    }

    /// Create an item from bounded in-memory bytes.
    pub fn new_bytes(
        bytes: Vec<u8>,
        label: Label,
        media_type: MediaType,
        source_name: impl Into<String>,
    ) -> AppResult<Self> {
        Self::new_bytes_with_limits(
            bytes,
            label,
            media_type,
            source_name,
            &DatasetLimits::default(),
        )
    }

    /// Create an item from bounded in-memory bytes with explicit limits.
    pub fn new_bytes_with_limits(
        bytes: Vec<u8>,
        label: Label,
        media_type: MediaType,
        source_name: impl Into<String>,
        limits: &DatasetLimits,
    ) -> AppResult<Self> {
        Ok(Self {
            payload: DataPayload::bytes(bytes, limits)?,
            label,
            media_type,
            source_name: source_name.into(),
            extension: ".jpg".to_string(),
            metadata: HashMap::new(),
            source_offset: None,
        })
    }

    /// Create an item from a file path for streaming large payloads.
    #[must_use]
    pub fn new_file(
        path: impl Into<PathBuf>,
        label: Label,
        media_type: MediaType,
        source_name: impl Into<String>,
    ) -> Self {
        Self {
            payload: DataPayload::file(path),
            label,
            media_type,
            source_name: source_name.into(),
            extension: ".bin".to_string(),
            metadata: HashMap::new(),
            source_offset: None,
        }
    }

    /// Borrow the item payload.
    #[must_use]
    pub fn payload(&self) -> &DataPayload {
        &self.payload
    }

    /// Replace the payload after validating it against explicit limits.
    pub fn try_with_payload(
        mut self,
        payload: DataPayload,
        limits: &DatasetLimits,
    ) -> AppResult<Self> {
        if payload.is_bytes() && payload.len()? > limits.max_in_memory_bytes as u64 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "in-memory dataset payload exceeds max_in_memory_bytes={}",
                    limits.max_in_memory_bytes
                ),
            ));
        }
        self.payload = payload;
        Ok(self)
    }

    /// Return the source-provided resume cursor after this item, if available.
    #[must_use]
    pub fn source_offset(&self) -> Option<usize> {
        self.source_offset
    }

    /// Attach a source-provided resume cursor.
    #[must_use]
    pub fn with_source_offset(mut self, offset: usize) -> Self {
        self.source_offset = Some(offset);
        self
    }

    /// Set the output extension.
    #[must_use]
    pub fn with_extension(mut self, ext: impl Into<String>) -> Self {
        self.extension = ext.into();
        self
    }

    /// Attach string metadata.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Validate the item against configured limits and path safety rules.
    pub fn validate(&self, limits: &DatasetLimits) -> AppResult<()> {
        validate_extension(&self.extension)?;
        let len = self.payload.len()?;
        if self.payload.is_bytes() && len > limits.max_in_memory_bytes as u64 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "in-memory dataset payload is {len} bytes, exceeding max_in_memory_bytes={}",
                    limits.max_in_memory_bytes
                ),
            ));
        }
        Ok(())
    }

    /// Write this item to `path` using the configured payload limits.
    pub fn write_to_path(&self, path: &Path, limits: &DatasetLimits) -> AppResult<u64> {
        self.validate(limits)?;
        self.payload.write_to_path(path, limits)
    }
}

fn validate_extension(extension: &str) -> AppResult<()> {
    let extension = extension.trim_start_matches('.');
    if extension.is_empty()
        || extension.contains('/')
        || extension.contains('\\')
        || extension == "."
        || extension == ".."
        || extension.contains("..")
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("invalid dataset item extension: {extension:?}"),
        ));
    }
    rskit_validation::input::validate_safe_path(extension)
}

impl crate::DatasetItem for DataItem {
    fn label(&self) -> Label {
        self.label
    }

    fn source_offset(&self) -> Option<usize> {
        self.source_offset
    }
}

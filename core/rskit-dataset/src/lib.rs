//! Dataset collection framework — streaming sources, transforms, targets, and schema validation.

#![warn(missing_docs)]

pub mod collector;
pub mod manifest;
pub mod record;
pub mod schema;
pub mod source;
pub mod stream;
pub mod target;
pub mod transform;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

pub use collector::{Collector, CollectorConfig, CollectorResult, NullProgress, ProgressCallback};
pub use manifest::{CacheStatus, Manifest, SourceEntry, SourceStats};
pub use record::{
    BoxRecordStream, CsvReader, CsvWriter, DatasetFormat, DatasetReader, DatasetRecord,
    DatasetWriter, JsonArrayReader, JsonArrayWriter, JsonLinesReader, JsonLinesWriter,
    filter_records, select_columns,
};
pub use schema::{DatasetSchema, validate_record};
pub use source::{BoxDataStream, Source};
pub use stream::DatasetStreamExt;
pub use target::{PublishResult, Target};
#[cfg(feature = "image-transform")]
pub use transform::ResizeTransform;
pub use transform::Transform;

/// Default threshold above which payloads should be represented as files.
pub const DEFAULT_MAX_IN_MEMORY_BYTES: usize = 8 * 1024 * 1024;

/// Runtime limits for dataset streaming and bounded materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetLimits {
    /// Largest payload that may be held in memory by bounded helpers.
    pub max_in_memory_bytes: usize,
    /// Bounded channel capacity for source/collector stream plumbing.
    pub stream_buffer: usize,
}

impl Default for DatasetLimits {
    fn default() -> Self {
        Self {
            max_in_memory_bytes: DEFAULT_MAX_IN_MEMORY_BYTES,
            stream_buffer: 64,
        }
    }
}

/// Binary classification label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u8)]
pub enum Label {
    /// Human-authored / real sample.
    Real = 0,
    /// AI-generated sample.
    AiGenerated = 1,
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Real => write!(f, "real"),
            Label::AiGenerated => write!(f, "ai"),
        }
    }
}

/// Supported media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MediaType {
    /// Image sample.
    Image,
    /// Text sample.
    Text,
    /// Audio sample.
    Audio,
    /// Video sample.
    Video,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaType::Image => write!(f, "image"),
            MediaType::Text => write!(f, "text"),
            MediaType::Audio => write!(f, "audio"),
            MediaType::Video => write!(f, "video"),
        }
    }
}

/// Payload for a single dataset item.
///
/// In-memory payloads can only be constructed through checked constructors so
/// large datasets do not accidentally bypass [`DatasetLimits`].
#[derive(Debug, Clone)]
pub struct DataPayload {
    kind: DataPayloadKind,
}

#[derive(Debug, Clone)]
enum DataPayloadKind {
    Bytes(Vec<u8>),
    File(PathBuf),
}

impl DataPayload {
    /// Create a bounded in-memory payload with explicit limits.
    pub fn bytes(bytes: Vec<u8>, limits: &DatasetLimits) -> AppResult<Self> {
        if bytes.len() > limits.max_in_memory_bytes {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "in-memory dataset payload is {} bytes, exceeding max_in_memory_bytes={}",
                    bytes.len(),
                    limits.max_in_memory_bytes
                ),
            ));
        }
        Ok(Self {
            kind: DataPayloadKind::Bytes(bytes),
        })
    }

    /// Create a bounded in-memory payload using [`DatasetLimits::default`].
    pub fn bytes_default(bytes: Vec<u8>) -> AppResult<Self> {
        Self::bytes(bytes, &DatasetLimits::default())
    }

    /// Create a file-backed payload for streaming large data.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: DataPayloadKind::File(path.into()),
        }
    }

    /// Returns true when this payload is memory-backed.
    #[must_use]
    pub fn is_bytes(&self) -> bool {
        matches!(self.kind, DataPayloadKind::Bytes(_))
    }

    /// Borrow the file path when this is a file-backed payload.
    #[must_use]
    pub fn as_file(&self) -> Option<&Path> {
        match &self.kind {
            DataPayloadKind::File(path) => Some(path),
            DataPayloadKind::Bytes(_) => None,
        }
    }

    /// Return the payload size in bytes when it can be determined.
    pub fn len(&self) -> AppResult<u64> {
        match &self.kind {
            DataPayloadKind::Bytes(bytes) => Ok(bytes.len() as u64),
            DataPayloadKind::File(path) => std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to stat payload file {}: {error}", path.display()),
                    )
                }),
        }
    }

    /// Return true when the payload has zero bytes.
    pub fn is_empty(&self) -> AppResult<bool> {
        self.len().map(|len| len == 0)
    }

    /// Read a payload into memory only if it is within configured bounds.
    pub fn read_bytes_bounded(&self, limits: &DatasetLimits) -> AppResult<Vec<u8>> {
        match &self.kind {
            DataPayloadKind::Bytes(bytes) => {
                if bytes.len() > limits.max_in_memory_bytes {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "dataset payload is {} bytes, exceeding max_in_memory_bytes={}",
                            bytes.len(),
                            limits.max_in_memory_bytes
                        ),
                    ));
                }
                Ok(bytes.clone())
            }
            DataPayloadKind::File(path) => read_file_bounded(path, limits.max_in_memory_bytes),
        }
    }

    /// Write the payload to `path`, streaming file payloads without materializing them.
    pub fn write_to_path(&self, path: &Path, limits: &DatasetLimits) -> AppResult<u64> {
        match &self.kind {
            DataPayloadKind::Bytes(bytes) => {
                if bytes.len() > limits.max_in_memory_bytes {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "in-memory dataset payload is {} bytes, exceeding max_in_memory_bytes={}",
                            bytes.len(),
                            limits.max_in_memory_bytes
                        ),
                    ));
                }
                std::fs::write(path, bytes).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to write dataset item {}: {error}", path.display()),
                    )
                })?;
                Ok(bytes.len() as u64)
            }
            DataPayloadKind::File(source) => {
                if is_same_file(source, path)? {
                    return self.len();
                }
                let mut input = std::fs::File::open(source).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to open payload file {}: {error}", source.display()),
                    )
                })?;
                let mut output = std::fs::File::create(path).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to create dataset item {}: {error}", path.display()),
                    )
                })?;
                std::io::copy(&mut input, &mut output).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "failed to stream payload {} to {}: {error}",
                            source.display(),
                            path.display()
                        ),
                    )
                })
            }
        }
    }
}

fn is_same_file(source: &Path, destination: &Path) -> AppResult<bool> {
    let source_metadata = std::fs::metadata(source).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to stat payload file {}: {error}", source.display()),
        )
    })?;
    let destination_metadata = match std::fs::metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to stat dataset item {}: {error}",
                    destination.display()
                ),
            ));
        }
    };

    if same_file_metadata(&source_metadata, &destination_metadata) {
        return Ok(true);
    }

    let source = std::fs::canonicalize(source).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize payload file {}: {error}",
                source.display()
            ),
        )
    })?;
    let destination = std::fs::canonicalize(destination).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize dataset item {}: {error}",
                destination.display()
            ),
        )
    })?;
    Ok(source == destination)
}

#[cfg(unix)]
fn same_file_metadata(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_metadata(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    matches!(
        (
            left.volume_serial_number(),
            left.file_index(),
            right.volume_serial_number(),
            right.file_index()
        ),
        (Some(left_volume), Some(left_index), Some(right_volume), Some(right_index))
            if left_volume == right_volume && left_index == right_index
    )
}

#[cfg(not(any(unix, windows)))]
fn same_file_metadata(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
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

fn read_file_bounded(path: &Path, max_bytes: usize) -> AppResult<Vec<u8>> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to open payload file {}: {error}", path.display()),
        )
    })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read payload file {}: {error}", path.display()),
            )
        })?;
    if bytes.len() > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "dataset payload exceeded max_in_memory_bytes={max_bytes} while reading {}",
                path.display()
            ),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_file_branches_read_write_and_same_file_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.bin");
        let copy = dir.path().join("copy.bin");
        let missing = dir.path().join("missing.bin");
        let blocked_destination = dir.path().join("blocked");
        std::fs::write(&source, b"payload").unwrap();
        std::fs::create_dir(&blocked_destination).unwrap();
        let limits = DatasetLimits {
            max_in_memory_bytes: 7,
            stream_buffer: 1,
        };
        let payload = DataPayload::file(&source);

        assert_eq!(payload.as_file(), Some(source.as_path()));
        assert_eq!(payload.len().unwrap(), 7);
        assert!(!payload.is_empty().unwrap());
        assert_eq!(payload.read_bytes_bounded(&limits).unwrap(), b"payload");
        assert_eq!(payload.write_to_path(&copy, &limits).unwrap(), 7);
        assert_eq!(std::fs::read(&copy).unwrap(), b"payload");
        assert_eq!(payload.write_to_path(&source, &limits).unwrap(), 7);

        let too_small = DatasetLimits {
            max_in_memory_bytes: 6,
            stream_buffer: 1,
        };
        assert_eq!(
            payload.read_bytes_bounded(&too_small).unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        assert_eq!(
            DataPayload::file(&missing)
                .write_to_path(&copy, &limits)
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
        assert_eq!(
            payload
                .write_to_path(&blocked_destination, &limits)
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
    }

    #[test]
    fn data_item_builders_validate_metadata_offsets_and_extensions() {
        let limits = DatasetLimits {
            max_in_memory_bytes: 3,
            stream_buffer: 1,
        };
        let item = DataItem::new(b"abc".to_vec(), Label::Real, MediaType::Text, "unit")
            .unwrap()
            .with_extension("txt")
            .with_metadata("kind", "fixture")
            .with_source_offset(42);

        assert_eq!(Label::Real.to_string(), "real");
        assert_eq!(Label::AiGenerated.to_string(), "ai");
        assert_eq!(MediaType::Image.to_string(), "image");
        assert_eq!(MediaType::Text.to_string(), "text");
        assert_eq!(MediaType::Audio.to_string(), "audio");
        assert_eq!(MediaType::Video.to_string(), "video");
        assert_eq!(item.source_offset(), Some(42));
        assert_eq!(
            item.metadata.get("kind").map(String::as_str),
            Some("fixture")
        );
        item.validate(&limits).unwrap();

        let memory_payload = DataPayload::bytes_default(vec![1, 2]).unwrap();
        assert!(memory_payload.is_bytes());
        assert_eq!(memory_payload.as_file(), None);
        assert_eq!(memory_payload.len().unwrap(), 2);
        assert!(!memory_payload.is_empty().unwrap());
        assert_eq!(
            memory_payload.read_bytes_bounded(&limits).unwrap(),
            vec![1, 2]
        );
        let output = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(
            memory_payload
                .write_to_path(output.path(), &limits)
                .unwrap(),
            2
        );
        assert_eq!(std::fs::read(output.path()).unwrap(), vec![1, 2]);

        let too_small = DatasetLimits {
            max_in_memory_bytes: 1,
            stream_buffer: 1,
        };
        assert_eq!(
            DataPayload::bytes(vec![1, 2], &too_small)
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            memory_payload
                .read_bytes_bounded(&too_small)
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            memory_payload
                .write_to_path(output.path(), &too_small)
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );

        let oversized = DataPayload::bytes(vec![1, 2, 3, 4], &DatasetLimits::default()).unwrap();
        assert_eq!(
            item.clone()
                .try_with_payload(oversized, &limits)
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            item.with_extension("../bad")
                .validate(&DatasetLimits::default())
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn data_item_validate_rejects_oversized_existing_payload_and_directory_reads() {
        let dir = tempfile::tempdir().unwrap();
        let limits = DatasetLimits {
            max_in_memory_bytes: 2,
            stream_buffer: 1,
        };
        let item = DataItem::new(b"abc".to_vec(), Label::Real, MediaType::Text, "unit").unwrap();

        assert_eq!(
            item.validate(&limits).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            DataPayload::file(dir.path())
                .read_bytes_bounded(&limits)
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
    }
}

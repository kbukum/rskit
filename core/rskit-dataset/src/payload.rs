use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::DatasetLimits;

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

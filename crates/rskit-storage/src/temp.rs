//! Managed temporary files and directories with auto-cleanup.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Managed temporary file. Deleted when the inner handle is dropped.
#[derive(Debug)]
pub struct TempFile {
    inner: tempfile::NamedTempFile,
}

impl TempFile {
    /// Create a new temporary file in the system temp directory.
    pub fn new() -> AppResult<Self> {
        let inner = tempfile::NamedTempFile::new().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp file: {e}"),
            )
        })?;
        Ok(Self { inner })
    }

    /// Create a temporary file with the given extension.
    pub fn with_extension(ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create temp file with extension .{ext}: {e}"),
                )
            })?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory.
    pub fn in_dir(dir: &Path) -> AppResult<Self> {
        let inner = tempfile::NamedTempFile::new_in(dir).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp file in {}: {e}", dir.display()),
            )
        })?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory with the given extension.
    pub fn in_dir_with_extension(dir: &Path, ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile_in(dir)
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create temp file in {} with extension .{ext}: {e}",
                        dir.display()
                    ),
                )
            })?;
        Ok(Self { inner })
    }

    /// The path to this temporary file.
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Convert this temp file into a [`super::FileSource`].
    pub fn into_source(self) -> super::FileSource {
        super::FileSource::Temp(self)
    }

    /// Create an independent copy of this temporary file.
    pub fn try_clone(&self) -> AppResult<Self> {
        let new = TempFile::new()?;
        std::fs::copy(self.path(), new.path()).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to clone temp file {} -> {}: {e}",
                    self.path().display(),
                    new.path().display()
                ),
            )
        })?;
        Ok(new)
    }

    /// Persist this temporary file to the given target path.
    /// The file will no longer be auto-deleted.
    pub fn persist(self, target: impl AsRef<Path>) -> AppResult<PathBuf> {
        let target = target.as_ref().to_path_buf();
        self.inner.persist(&target).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to persist temp file to {}: {e}", target.display()),
            )
        })?;
        Ok(target)
    }
}

/// Managed temporary directory. All contents are cleaned up on drop.
pub struct TempDir {
    inner: tempfile::TempDir,
}

impl TempDir {
    /// Create a new temporary directory.
    pub fn new() -> AppResult<Self> {
        let inner = tempfile::TempDir::new().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp dir: {e}"),
            )
        })?;
        Ok(Self { inner })
    }

    /// The path to this temporary directory.
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Create a named file inside this temp directory.
    pub fn create_file(&self, name: &str) -> AppResult<TempFile> {
        let inner = tempfile::Builder::new()
            .prefix(name)
            .tempfile_in(self.path())
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create file {name} in temp dir: {e}"),
                )
            })?;
        Ok(TempFile { inner })
    }

    /// Create a file with the given extension inside this temp directory.
    pub fn create_file_with_extension(&self, ext: &str) -> AppResult<TempFile> {
        TempFile::in_dir_with_extension(self.path(), ext)
    }
}

impl std::fmt::Debug for TempDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TempDir")
            .field("path", &self.inner.path())
            .finish()
    }
}

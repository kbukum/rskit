//! Temporary file and path helpers.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::path::parent_dir;

static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(1);

/// Managed temporary file. Deleted when the inner handle is dropped.
#[derive(Debug)]
pub struct TempFile {
    inner: tempfile::NamedTempFile,
}

impl TempFile {
    /// Create a new temporary file in the system temp directory.
    pub fn new() -> AppResult<Self> {
        let inner = tempfile::NamedTempFile::new().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp file: {error}"),
            )
        })?;
        Ok(Self { inner })
    }

    /// Create a temporary file with the given extension.
    pub fn with_extension(ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create temp file with extension .{ext}: {error}"),
                )
            })?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory.
    pub fn in_dir(dir: &Path) -> AppResult<Self> {
        let inner = tempfile::NamedTempFile::new_in(dir).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp file in {}: {error}", dir.display()),
            )
        })?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory with the given extension.
    pub fn in_dir_with_extension(dir: &Path, ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile_in(dir)
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create temp file in {} with extension .{ext}: {error}",
                        dir.display()
                    ),
                )
            })?;
        Ok(Self { inner })
    }

    /// The path to this temporary file.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Create an independent copy of this temporary file.
    pub fn try_clone(&self) -> AppResult<Self> {
        let new = Self::new()?;
        std::fs::copy(self.path(), new.path())
            .map_err(|error| AppError::internal(error).context("clone temp file"))?;
        Ok(new)
    }

    /// Persist this temporary file to the given target path.
    ///
    /// The file will no longer be auto-deleted.
    pub fn persist(self, target: impl AsRef<Path>) -> AppResult<PathBuf> {
        let target = target.as_ref().to_path_buf();
        self.inner.persist(&target).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to persist temp file to {}: {error}",
                    target.display()
                ),
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
        let inner = tempfile::TempDir::new().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create temp dir: {error}"),
            )
        })?;
        Ok(Self { inner })
    }

    /// The path to this temporary directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Create a child path within this temp directory.
    pub fn child(&self, rel_path: impl AsRef<Path>) -> AppResult<PathBuf> {
        crate::safe_join(self.path(), rel_path.as_ref())
            .map_err(|error| AppError::new(ErrorCode::InvalidInput, error.to_string()))
    }

    /// Write a file at a relative path within this temp directory.
    pub fn write_file(&self, rel_path: impl AsRef<Path>, content: &[u8]) -> AppResult<PathBuf> {
        let path = self.child(rel_path)?;
        if let Some(parent) = parent_dir(&path) {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create parent dirs: {error}"),
                )
            })?;
        }
        std::fs::write(&path, content).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write file '{}': {error}", path.display()),
            )
        })?;
        Ok(path)
    }

    /// Create a named file inside this temp directory.
    pub fn create_file(&self, name: &str) -> AppResult<TempFile> {
        let inner = tempfile::Builder::new()
            .prefix(name)
            .tempfile_in(self.path())
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create file {name} in temp dir: {error}"),
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

/// Build a collision-resistant temp path next to a destination path.
///
/// The function only constructs a path; callers still own creation mode,
/// streaming writes, fsync/flush, and final rename/persist policy.
#[must_use]
pub fn sibling_temp_path(dest: &Path, prefix: &str, suffix: &str) -> PathBuf {
    let parent = parent_dir(dest).unwrap_or_else(|| Path::new("."));
    let sequence = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    parent.join(format!(
        ".{prefix}-{}-{nanos}-{sequence}{suffix}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{TempDir, TempFile, sibling_temp_path};

    #[test]
    fn sibling_temp_paths_are_unique_and_next_to_destination() {
        let dest = Path::new("/tmp/output.txt");
        let first = sibling_temp_path(dest, "download", ".tmp");
        let second = sibling_temp_path(dest, "download", ".tmp");

        assert_ne!(first, second);
        assert_eq!(first.parent(), dest.parent());
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("download")
        );
    }

    #[test]
    fn temp_dir_child_rejects_traversal() {
        let dir = TempDir::new().unwrap();
        assert!(dir.child("../escape").is_err());
    }

    #[test]
    fn temp_dir_write_file_creates_parents() {
        let dir = TempDir::new().unwrap();
        let path = dir.write_file("a/b.txt", b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello");
    }

    #[test]
    fn temp_file_can_be_cloned() {
        let file = TempFile::new().unwrap();
        std::fs::write(file.path(), b"data").unwrap();
        let cloned = file.try_clone().unwrap();
        assert_eq!(std::fs::read(cloned.path()).unwrap(), b"data");
    }
}

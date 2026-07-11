//! Managed temporary files and directories with auto-cleanup.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::sync_io::file;

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
        file::copy(self.path(), new.path()).map_err(|error| error.context("clone temp file"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_file_clone_source_and_persist_keep_expected_contents() {
        let file = TempFile::with_extension("txt").unwrap();
        std::fs::write(file.path(), b"hello").unwrap();

        let clone = file.try_clone().unwrap();
        assert_eq!(std::fs::read(clone.path()).unwrap(), b"hello");
        assert!(matches!(clone.into_source(), crate::FileSource::Temp(_)));

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("persisted.txt");
        let persisted = file.persist(&target).unwrap();
        assert_eq!(persisted, target);
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn temp_dir_creates_files_and_debug_exposes_path() {
        let dir = TempDir::new().unwrap();
        let named = dir.create_file("named").unwrap();
        let with_ext = dir.create_file_with_extension("bin").unwrap();
        let in_dir = TempFile::in_dir(dir.path()).unwrap();
        let in_dir_with_ext = TempFile::in_dir_with_extension(dir.path(), "txt").unwrap();

        assert!(named.path().starts_with(dir.path()));
        assert!(with_ext.path().starts_with(dir.path()));
        assert!(in_dir.path().starts_with(dir.path()));
        assert_eq!(
            in_dir_with_ext
                .path()
                .extension()
                .and_then(|ext| ext.to_str()),
            Some("txt")
        );
        assert!(
            format!("{dir:?}").contains(dir.path().file_name().unwrap().to_string_lossy().as_ref())
        );
    }

    #[test]
    fn temp_file_new_creates_existing_path() {
        let file = TempFile::new().unwrap();
        assert!(file.path().exists());
    }

    #[test]
    fn temp_file_reports_invalid_directory_errors() {
        let missing =
            std::env::temp_dir().join(format!("rskit-storage-missing-{}", std::process::id()));
        let error = TempFile::in_dir(&missing).unwrap_err();

        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(error.to_string().contains("failed to create temp file"));
    }
}

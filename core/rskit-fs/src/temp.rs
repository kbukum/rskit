//! Temporary file and path helpers.
#![allow(clippy::needless_pass_by_value)]

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
        let inner = tempfile::NamedTempFile::new().map_err(create_temp_file_error)?;
        Ok(Self { inner })
    }

    /// Create a temporary file with the given extension.
    pub fn with_extension(ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .map_err(|error| create_temp_file_with_extension_error(ext, error))?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory.
    pub fn in_dir(dir: &Path) -> AppResult<Self> {
        let inner = tempfile::NamedTempFile::new_in(dir)
            .map_err(|error| create_temp_file_in_dir_error(dir, error))?;
        Ok(Self { inner })
    }

    /// Create a temporary file in the given directory with the given extension.
    pub fn in_dir_with_extension(dir: &Path, ext: &str) -> AppResult<Self> {
        let inner = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile_in(dir)
            .map_err(|error| create_temp_file_in_dir_with_extension_error(dir, ext, error))?;
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
        let inner = tempfile::TempDir::new().map_err(create_temp_dir_error)?;
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
        let parent = parent_dir(&path).unwrap_or_else(|| self.path());
        std::fs::create_dir_all(parent).map_err(create_parent_dirs_error)?;
        std::fs::write(&path, content).map_err(|error| write_temp_dir_file_error(&path, error))?;
        Ok(path)
    }

    /// Create a named file inside this temp directory.
    pub fn create_file(&self, name: &str) -> AppResult<TempFile> {
        let inner = tempfile::Builder::new()
            .prefix(name)
            .tempfile_in(self.path())
            .map_err(|error| create_named_temp_dir_file_error(name, error))?;
        Ok(TempFile { inner })
    }

    /// Create a file with the given extension inside this temp directory.
    pub fn create_file_with_extension(&self, ext: &str) -> AppResult<TempFile> {
        TempFile::in_dir_with_extension(self.path(), ext)
    }
}

fn create_temp_file_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create temp file: {error}"),
    )
}

fn create_temp_file_with_extension_error(ext: &str, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create temp file with extension .{ext}: {error}"),
    )
}

fn create_temp_file_in_dir_error(dir: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create temp file in {}: {error}", dir.display()),
    )
}

fn create_temp_file_in_dir_with_extension_error(
    dir: &Path,
    ext: &str,
    error: std::io::Error,
) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to create temp file in {} with extension .{ext}: {error}",
            dir.display()
        ),
    )
}

fn create_temp_dir_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create temp dir: {error}"),
    )
}

fn create_parent_dirs_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create parent dirs: {error}"),
    )
}

fn write_temp_dir_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to write file '{}': {error}", path.display()),
    )
}

fn create_named_temp_dir_file_error(name: &str, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create file {name} in temp dir: {error}"),
    )
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
/// `prefix` and `suffix` are sanitized before interpolation so the generated
/// file name remains a single path component under `dest`'s parent directory.
///
/// The function only constructs a path; callers still own creation mode,
/// streaming writes, fsync/flush, and final rename/persist policy.
#[must_use]
pub fn sibling_temp_path(dest: &Path, prefix: &str, suffix: &str) -> PathBuf {
    let parent = parent_dir(dest).unwrap_or_else(|| Path::new("."));
    let prefix = sanitize_temp_prefix(prefix);
    let suffix = sanitize_temp_suffix(suffix);
    let sequence = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    parent.join(format!(
        ".{prefix}-{}-{nanos}-{sequence}{suffix}",
        std::process::id()
    ))
}

fn sanitize_temp_prefix(value: &str) -> String {
    sanitize_temp_affix(value, false)
}

fn sanitize_temp_suffix(value: &str) -> String {
    sanitize_temp_affix(value, true)
}

fn sanitize_temp_affix(value: &str, allow_dot: bool) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let mut previous_dot = false;

    for character in value.chars() {
        let replacement = match character {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' => character,
            '.' if allow_dot && !previous_dot => '.',
            _ => '_',
        };
        previous_dot = replacement == '.';
        sanitized.push(replacement);
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        TempDir, TempFile, create_named_temp_dir_file_error, create_parent_dirs_error,
        create_temp_dir_error, create_temp_file_error, create_temp_file_in_dir_error,
        create_temp_file_in_dir_with_extension_error, create_temp_file_with_extension_error,
        sibling_temp_path, write_temp_dir_file_error,
    };

    #[test]
    fn temp_file_constructors_create_files() {
        let file = TempFile::new().unwrap();
        assert!(file.path().exists());

        let with_ext = TempFile::with_extension("txt").unwrap();
        assert!(
            with_ext
                .path()
                .ends_with(with_ext.path().file_name().unwrap())
        );
        assert!(with_ext.path().to_string_lossy().ends_with(".txt"));

        let dir = TempDir::new().unwrap();
        let in_dir = TempFile::in_dir(dir.path()).unwrap();
        assert_eq!(in_dir.path().parent(), Some(dir.path()));

        let in_dir_with_ext = TempFile::in_dir_with_extension(dir.path(), "log").unwrap();
        assert_eq!(in_dir_with_ext.path().parent(), Some(dir.path()));
        assert!(in_dir_with_ext.path().to_string_lossy().ends_with(".log"));
    }

    #[test]
    fn temp_file_constructors_report_invalid_directories() {
        let dir = TempDir::new().unwrap();
        let file = dir.write_file("file.txt", b"hello").unwrap();

        assert!(TempFile::in_dir(&file).is_err());
        assert!(TempFile::in_dir_with_extension(&file, "txt").is_err());
    }

    #[test]
    fn temp_error_builders_include_context() {
        let dir = Path::new("dir");
        let file = Path::new("dir/file.txt");
        let err = || std::io::Error::other("boom");

        assert!(
            create_temp_file_error(err())
                .to_string()
                .contains("temp file")
        );
        assert!(
            create_temp_file_with_extension_error("txt", err())
                .to_string()
                .contains(".txt")
        );
        assert!(
            create_temp_file_in_dir_error(dir, err())
                .to_string()
                .contains("in dir")
        );
        assert!(
            create_temp_file_in_dir_with_extension_error(dir, "txt", err())
                .to_string()
                .contains(".txt")
        );
        assert!(
            create_temp_dir_error(err())
                .to_string()
                .contains("temp dir")
        );
        assert!(
            create_parent_dirs_error(err())
                .to_string()
                .contains("parent dirs")
        );
        assert!(
            write_temp_dir_file_error(file, err())
                .to_string()
                .contains("write file")
        );
        assert!(
            create_named_temp_dir_file_error("name", err())
                .to_string()
                .contains("name")
        );
    }

    #[test]
    fn temp_file_persist_moves_file() {
        let dir = TempDir::new().unwrap();
        let file = TempFile::in_dir(dir.path()).unwrap();
        std::fs::write(file.path(), b"persisted").unwrap();
        let target = dir.child("persisted.txt").unwrap();

        let persisted = file.persist(&target).unwrap();

        assert_eq!(persisted, target);
        assert_eq!(std::fs::read_to_string(persisted).unwrap(), "persisted");
    }

    #[test]
    fn temp_file_persist_reports_errors() {
        let dir = TempDir::new().unwrap();
        let file = TempFile::in_dir(dir.path()).unwrap();
        let target = dir.child("missing/target.txt").unwrap();

        assert!(file.persist(target).is_err());
    }

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
    fn sibling_temp_path_sanitizes_affixes() {
        let dest = Path::new("/tmp/output.txt");
        let path = sibling_temp_path(dest, "../escape", "/..\\payload");
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert_eq!(path.parent(), dest.parent());
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains('\\'));
        assert!(!file_name.contains(".."));
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
    fn temp_dir_write_file_reports_errors() {
        let dir = TempDir::new().unwrap();
        dir.write_file("file.txt", b"hello").unwrap();

        assert!(dir.write_file("file.txt/child.txt", b"nope").is_err());
    }

    #[test]
    fn temp_dir_create_file_helpers_create_files() {
        let dir = TempDir::new().unwrap();
        let named = dir.create_file("named").unwrap();
        assert_eq!(named.path().parent(), Some(dir.path()));
        assert!(
            named
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("named")
        );

        let with_extension = dir.create_file_with_extension("txt").unwrap();
        assert_eq!(with_extension.path().parent(), Some(dir.path()));
        assert!(with_extension.path().to_string_lossy().ends_with(".txt"));
    }

    #[test]
    fn temp_dir_debug_includes_path() {
        let dir = TempDir::new().unwrap();
        let debug = format!("{dir:?}");

        assert!(debug.contains("TempDir"));
        assert!(debug.contains(&dir.path().display().to_string()));
    }

    #[test]
    fn temp_file_can_be_cloned() {
        let file = TempFile::new().unwrap();
        std::fs::write(file.path(), b"data").unwrap();
        let cloned = file.try_clone().unwrap();
        assert_eq!(std::fs::read(cloned.path()).unwrap(), b"data");
    }
}

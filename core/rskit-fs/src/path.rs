//! Safe path helpers.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Error returned when a relative path escapes its expected root.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SafePathError {
    /// Absolute paths are not accepted for root-relative operations.
    Absolute,
    /// Parent-directory components are not accepted for root-relative operations.
    ParentDir,
    /// Platform-specific path prefixes are not accepted for root-relative operations.
    Prefix,
}

impl fmt::Display for SafePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute => f.write_str("path must be relative, not absolute"),
            Self::ParentDir => f.write_str("path must not contain '..' segments"),
            Self::Prefix => f.write_str("path must not contain a platform path prefix"),
        }
    }
}

impl std::error::Error for SafePathError {}

/// Validate that `path` is safe to join under a caller-owned root directory.
pub fn validate_relative_path(path: &Path) -> Result<(), SafePathError> {
    for component in path.components() {
        match component {
            Component::RootDir => return Err(SafePathError::Absolute),
            Component::ParentDir => return Err(SafePathError::ParentDir),
            Component::Prefix(_) => return Err(SafePathError::Prefix),
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    if path.is_absolute() {
        return Err(SafePathError::Absolute);
    }
    Ok(())
}

/// Join a caller-owned root with a validated relative path.
pub fn safe_join(root: &Path, rel_path: impl AsRef<Path>) -> Result<PathBuf, SafePathError> {
    let rel_path = rel_path.as_ref();
    validate_relative_path(rel_path)?;
    Ok(root.join(rel_path))
}

/// Return an absolute path without requiring the path to exist.
pub fn absolute(path: &Path) -> AppResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| AppError::new(ErrorCode::Internal, format!("failed to read cwd: {error}")))
}

/// Canonicalize a path by resolving symlinks and normalizing components.
pub async fn canonicalize(path: &Path) -> AppResult<PathBuf> {
    tokio::fs::canonicalize(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to canonicalize '{}': {error}", path.display()),
        )
    })
}

/// Return the non-empty parent directory for `path`.
#[must_use]
pub fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{SafePathError, absolute, safe_join, validate_relative_path};

    #[test]
    fn validates_safe_relative_paths() {
        assert!(validate_relative_path(Path::new("a/b.txt")).is_ok());
        assert!(validate_relative_path(Path::new("./a/b.txt")).is_ok());
    }

    #[test]
    fn rejects_absolute_paths() {
        assert_eq!(
            validate_relative_path(Path::new("/etc/passwd")).unwrap_err(),
            SafePathError::Absolute
        );
    }

    #[test]
    fn rejects_parent_dir_paths() {
        assert_eq!(
            validate_relative_path(Path::new("../escape")).unwrap_err(),
            SafePathError::ParentDir
        );
    }

    #[test]
    fn safe_join_keeps_paths_under_root() {
        assert_eq!(
            safe_join(Path::new("/root"), "a/b.txt").unwrap(),
            Path::new("/root").join("a/b.txt")
        );
    }

    #[test]
    fn absolute_resolves_relative_paths() {
        let path = absolute(Path::new("a/b.txt")).unwrap();
        assert!(path.is_absolute());
        assert!(path.ends_with("a/b.txt"));
    }
}

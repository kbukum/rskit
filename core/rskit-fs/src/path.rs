//! Safe path helpers.

use std::ffi::OsString;
use std::fmt;
use std::io::ErrorKind;
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
            #[cfg(windows)]
            Component::Prefix(_) => return Err(SafePathError::Prefix),
            #[cfg(not(windows))]
            Component::Prefix(_) | Component::CurDir | Component::Normal(_) => {}
            #[cfg(windows)]
            Component::CurDir | Component::Normal(_) => {}
        }
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
pub fn canonicalize(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to canonicalize '{}': {error}", path.display()),
        )
    })
}

/// Canonicalize an existing `path` and reject it when it resolves outside `root`.
///
/// Use this for existing untrusted file paths before handing them to lower-level IO
/// or subprocess APIs. Both `root` and `path` are resolved through the filesystem so
/// symlink escapes are rejected. Relative `path` values are interpreted under `root`;
/// absolute `path` values are accepted only when their canonical destination is still
/// within `root`.
///
/// # Errors
///
/// Returns an error when `root` or `path` cannot be canonicalized, or when `path`
/// resolves outside the canonical root.
pub fn confine_existing_path(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = canonicalize(root)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let candidate = canonicalize(&candidate)?;
    ensure_confined(&root, &candidate)?;
    Ok(candidate)
}

/// Resolve `path` under `root` and reject escapes, allowing the final path to be missing.
///
/// This is intended for output paths. The nearest existing ancestor is canonicalized to catch
/// symlink escapes before new directories or files are created. Relative `path` values are
/// interpreted under `root`; absolute `path` values are accepted only when their resolved
/// existing ancestor remains within `root`.
///
/// # Errors
///
/// Returns an error when `root` cannot be canonicalized, no existing ancestor can be found,
/// an existing ancestor resolves outside `root`, or a missing path segment is unsafe.
pub fn confine_path(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = canonicalize(root)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let (existing, missing) = existing_ancestor_and_missing_suffix(&candidate)?;
    let existing = canonicalize_existing_ancestor(&existing)?;
    ensure_confined(&root, &existing)?;

    let resolved = append_safe_missing_suffix(existing, missing)?;
    ensure_confined(&root, &resolved)?;
    Ok(resolved)
}

fn existing_ancestor_and_missing_suffix(path: &Path) -> AppResult<(PathBuf, Vec<OsString>)> {
    let mut missing = Vec::new();
    let mut current = path.to_path_buf();
    while !exists_without_following_symlinks(&current)? {
        let Some(name) = current.file_name().map(OsString::from) else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("no existing ancestor for '{}'", path.display()),
            ));
        };
        missing.push(name);
        let Some(parent) = current.parent() else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("no existing ancestor for '{}'", path.display()),
            ));
        };
        current = parent.to_path_buf();
    }
    missing.reverse();
    Ok((current, missing))
}

fn exists_without_following_symlinks(path: &Path) -> AppResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to inspect '{}': {error}", path.display()),
        )),
    }
}

fn canonicalize_existing_ancestor(path: &Path) -> AppResult<PathBuf> {
    canonicalize(path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "existing path ancestor '{}' cannot be resolved: {}",
                path.display(),
                error.message()
            ),
        )
    })
}

fn append_safe_missing_suffix(mut base: PathBuf, missing: Vec<OsString>) -> AppResult<PathBuf> {
    for segment in missing {
        let segment_path = Path::new(&segment);
        validate_relative_path(segment_path).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "path segment '{}' is not safe: {error}",
                    segment_path.display()
                ),
            )
        })?;
        let mut components = segment_path.components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("path segment '{}' is not safe", segment_path.display()),
            ));
        }
        base.push(segment);
    }
    Ok(base)
}

fn ensure_confined(root: &Path, path: &Path) -> AppResult<()> {
    if path.starts_with(root) {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "path '{}' resolves outside confined root '{}'",
            path.display(),
            root.display()
        ),
    ))
}

/// Return the non-empty parent directory for `path`.
#[must_use]
pub fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use rskit_errors::ErrorCode;

    use super::{
        SafePathError, absolute, append_safe_missing_suffix, canonicalize, confine_existing_path,
        confine_path, safe_join, validate_relative_path,
    };

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
    fn displays_safe_path_errors() {
        assert_eq!(
            SafePathError::Absolute.to_string(),
            "path must be relative, not absolute"
        );
        assert_eq!(
            SafePathError::ParentDir.to_string(),
            "path must not contain '..' segments"
        );
        assert_eq!(
            SafePathError::Prefix.to_string(),
            "path must not contain a platform path prefix"
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

    #[test]
    fn absolute_returns_absolute_paths_unchanged() {
        let path = Path::new("/tmp/a.txt");
        assert_eq!(absolute(path).unwrap(), path);
    }

    #[test]
    fn canonicalize_resolves_existing_paths_and_reports_missing() {
        let dir = crate::TempDir::new().unwrap();
        let file = dir.write_file("file.txt", b"hello").unwrap();

        assert_eq!(
            canonicalize(&file).unwrap(),
            std::fs::canonicalize(&file).unwrap()
        );
        assert!(canonicalize(&dir.child("missing.txt").unwrap()).is_err());
    }

    #[test]
    fn confines_existing_paths_under_root() {
        let dir = crate::TempDir::new().unwrap();
        let file = dir.write_file("nested/file.txt", b"hello").unwrap();

        let confined = confine_existing_path(dir.path(), Path::new("nested/file.txt")).unwrap();

        assert_eq!(confined, std::fs::canonicalize(file).unwrap());
    }

    #[test]
    fn rejects_existing_paths_outside_root() {
        let root = crate::TempDir::new().unwrap();
        let outside = crate::TempDir::new().unwrap();
        let file = outside.write_file("file.txt", b"hello").unwrap();

        let error = confine_existing_path(root.path(), &file).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn confines_missing_output_paths_under_existing_parent() {
        let dir = crate::TempDir::new().unwrap();

        let confined = confine_path(dir.path(), Path::new("nested/output.txt")).unwrap();

        assert!(confined.starts_with(std::fs::canonicalize(dir.path()).unwrap()));
        assert!(confined.ends_with("nested/output.txt"));
    }

    #[test]
    fn rejects_curdir_missing_path_segments() {
        let dir = crate::TempDir::new().unwrap();

        let error = append_safe_missing_suffix(dir.path().to_path_buf(), vec![OsString::from(".")])
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_missing_paths_below_symlink_escape() {
        let root = crate::TempDir::new().unwrap();
        let outside = crate::TempDir::new().unwrap();
        let link = root.child("link").unwrap();
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();

        let error = confine_path(root.path(), Path::new("link/output.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_missing_paths_below_broken_symlink() {
        let root = crate::TempDir::new().unwrap();
        let link = root.child("broken-link").unwrap();
        let target = root.child("missing-target").unwrap();
        std::os::unix::fs::symlink(target, &link).unwrap();

        let error = confine_path(root.path(), Path::new("broken-link/output.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}

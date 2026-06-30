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
    /// The path is empty (has no components).
    Empty,
}

impl fmt::Display for SafePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute => f.write_str("path must be relative, not absolute"),
            Self::ParentDir => f.write_str("path must not contain '..' segments"),
            Self::Prefix => f.write_str("path must not contain a platform path prefix"),
            Self::Empty => f.write_str("path must not be empty"),
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

/// Normalize a repo-relative path to a canonical form, rejecting traversal.
///
/// Strips `.` (current-directory) components so semantically equal inputs
/// (`a/b` and `a/./b`) share one canonical value, while rejecting absolute,
/// `..`, prefixed, or empty paths. A path consisting solely of `.` components
/// collapses to the repo root `.`. Use this for identity-bearing repo-relative
/// paths (module roots, manifests) that must be canonical and confined before
/// being joined to a root.
///
/// # Errors
///
/// Returns [`SafePathError`] when the path is empty, absolute, prefixed, or
/// contains a `..` segment.
pub fn normalize_repo_relative_path(path: impl AsRef<Path>) -> Result<PathBuf, SafePathError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(SafePathError::Empty);
    }
    validate_relative_path(path)?;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(SafePathError::ParentDir),
            Component::RootDir => return Err(SafePathError::Absolute),
            Component::Prefix(_) => return Err(SafePathError::Prefix),
        }
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    Ok(normalized)
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

/// Search `start` and each of its ancestor directories for a regular file named
/// `file_name`, returning the path to the nearest match.
///
/// This is the canonical "find the nearest config file" ascent: a loader locates
/// a project manifest or dotenv file by walking up from a starting directory
/// (typically the current working directory) until the file is found or the
/// filesystem root is reached. The first ancestor that contains a regular file
/// named `file_name` wins, so a nested directory's file shadows one higher up.
///
/// `file_name` is normally a bare filename; a multi-component relative path is
/// joined to each ancestor as-is. Symlinks are followed (a symlink to a regular
/// file matches). Returns `None` when no ancestor contains the file.
#[must_use]
pub fn find_in_ancestors(start: &Path, file_name: impl AsRef<Path>) -> Option<PathBuf> {
    let file_name = file_name.as_ref();
    let mut dir = start;
    loop {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
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

/// Resolve `root` against a base directory when relative, then canonicalize it.
///
/// Common when a config or manifest file declares a `root` that is either
/// absolute or relative to the file's own directory. A relative `root` is joined
/// to `base_dir`; an absolute `root` is used as-is. The result is canonicalized,
/// so the returned path always exists (canonicalization resolves symlinks and
/// requires the target to exist). When `root` is `None`, the current directory
/// (`"."`) is resolved against `base_dir`.
///
/// `field` names the source field for error reporting.
///
/// This resolves and canonicalizes but does not confine: it does not reject a
/// `root` that escapes `base_dir`. Use [`confine_path`] or [`confine_existing_path`]
/// when the resolved path must stay within a trust boundary.
///
/// # Errors
///
/// Returns [`AppError`] when the resolved path cannot be canonicalized (for
/// example, it does not exist), with the underlying cause preserved.
pub fn resolve_root_relative_to(
    field: &str,
    base_dir: &Path,
    root: Option<&Path>,
) -> AppResult<PathBuf> {
    let root = root.unwrap_or_else(|| Path::new("."));
    let resolved = if root.is_absolute() {
        root.to_path_buf()
    } else {
        base_dir.join(root)
    };
    canonicalize(&resolved).map_err(|error| {
        AppError::invalid_input(
            field,
            format!("failed to resolve {field} '{}'", resolved.display()),
        )
        .with_cause(error)
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
/// Returns an error when `root` or `path` cannot be canonicalized, when `root`
/// is not a directory, or when `path` resolves outside the canonical root.
pub fn confine_existing_path(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = canonicalize_directory_root(root)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let candidate = canonicalize_confined_input(&candidate, "confined path")?;
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
/// Returns an error when `root` cannot be canonicalized, `root` is not a directory, no existing
/// ancestor can be found, an existing ancestor resolves outside `root`, a missing suffix would be
/// appended below a non-directory ancestor, or a missing path segment is unsafe.
pub fn confine_path(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = canonicalize_directory_root(root)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let (existing, missing) = existing_ancestor_and_missing_suffix(&candidate)?;
    let existing = canonicalize_existing_ancestor(&existing)?;
    ensure_confined(&root, &existing)?;
    ensure_directory_for_missing_suffix(&existing, &missing)?;

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

fn canonicalize_directory_root(root: &Path) -> AppResult<PathBuf> {
    let root = canonicalize_confined_input(root, "confined root")?;
    let metadata = std::fs::metadata(&root).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to inspect confined root '{}': {error}",
                root.display()
            ),
        )
    })?;
    if metadata.is_dir() {
        return Ok(root);
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!("confined root '{}' is not a directory", root.display()),
    ))
}

fn canonicalize_confined_input(path: &Path, label: &str) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        AppError::new(
            confined_canonicalize_error_code(error.kind()),
            format!(
                "failed to canonicalize {label} '{}': {error}",
                path.display()
            ),
        )
    })
}

const fn confined_canonicalize_error_code(kind: ErrorKind) -> ErrorCode {
    match kind {
        ErrorKind::NotFound => ErrorCode::NotFound,
        ErrorKind::InvalidInput | ErrorKind::NotADirectory => ErrorCode::InvalidInput,
        _ => ErrorCode::Internal,
    }
}

fn exists_without_following_symlinks(path: &Path) -> AppResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(false)
        }
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to inspect '{}': {error}", path.display()),
        )),
    }
}

fn canonicalize_existing_ancestor(path: &Path) -> AppResult<PathBuf> {
    canonicalize_confined_input(path, "existing path ancestor").map_err(|error| {
        AppError::new(
            error.code(),
            format!(
                "existing path ancestor '{}' cannot be resolved: {}",
                path.display(),
                error.message()
            ),
        )
    })
}

fn ensure_directory_for_missing_suffix(existing: &Path, missing: &[OsString]) -> AppResult<()> {
    if missing.is_empty() {
        return Ok(());
    }
    let metadata = std::fs::metadata(existing).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to inspect existing path ancestor '{}': {error}",
                existing.display()
            ),
        )
    })?;
    if metadata.is_dir() {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "existing path ancestor '{}' is not a directory",
            existing.display()
        ),
    ))
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
        confine_path, find_in_ancestors, normalize_repo_relative_path, resolve_root_relative_to,
        safe_join, validate_relative_path,
    };

    #[test]
    fn normalize_repo_relative_strips_curdir_and_rejects_traversal() {
        assert_eq!(
            normalize_repo_relative_path("core/./errors").unwrap(),
            Path::new("core/errors")
        );
        assert_eq!(normalize_repo_relative_path(".").unwrap(), Path::new("."));
        assert_eq!(
            normalize_repo_relative_path("").unwrap_err(),
            SafePathError::Empty
        );
        assert_eq!(
            normalize_repo_relative_path("core/../etc").unwrap_err(),
            SafePathError::ParentDir
        );
        #[cfg(unix)]
        assert_eq!(
            normalize_repo_relative_path("/abs").unwrap_err(),
            SafePathError::Absolute
        );
    }

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
    fn rejects_missing_existing_paths_as_not_found() {
        let root = crate::TempDir::new().unwrap();

        let error = confine_existing_path(root.path(), Path::new("missing.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::NotFound);
    }

    #[test]
    fn rejects_missing_confined_roots_as_not_found() {
        let dir = crate::TempDir::new().unwrap();
        let missing_root = dir.child("missing-root").unwrap();

        let error = confine_existing_path(&missing_root, Path::new("file.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::NotFound);
    }

    #[test]
    fn rejects_file_root_for_existing_paths() {
        let dir = crate::TempDir::new().unwrap();
        let root_file = dir.write_file("root.txt", b"not a dir").unwrap();

        let error = confine_existing_path(&root_file, Path::new("child.txt")).unwrap_err();

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
    fn rejects_file_root_for_output_paths() {
        let dir = crate::TempDir::new().unwrap();
        let root_file = dir.write_file("root.txt", b"not a dir").unwrap();

        let error = confine_path(&root_file, Path::new("output.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn rejects_missing_output_paths_below_existing_file() {
        let dir = crate::TempDir::new().unwrap();
        dir.write_file("file.txt", b"not a dir").unwrap();

        let error = confine_path(dir.path(), Path::new("file.txt/output.txt")).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
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

        assert_eq!(error.code(), ErrorCode::NotFound);
    }

    #[test]
    fn resolve_root_defaults_to_base_dir() {
        let dir = crate::TempDir::new().unwrap();

        let root = resolve_root_relative_to("root", dir.path(), None).unwrap();

        assert_eq!(root, canonicalize(dir.path()).unwrap());
    }

    #[test]
    fn resolve_root_joins_relative_against_base_dir() {
        let dir = crate::TempDir::new().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();

        let root =
            resolve_root_relative_to("root", dir.path(), Some(Path::new("workspace"))).unwrap();

        assert_eq!(root, canonicalize(&workspace).unwrap());
    }

    #[test]
    fn resolve_root_accepts_absolute_root() {
        let base = crate::TempDir::new().unwrap();
        let target = crate::TempDir::new().unwrap();

        let root = resolve_root_relative_to("root", base.path(), Some(target.path())).unwrap();

        assert_eq!(root, canonicalize(target.path()).unwrap());
    }

    #[test]
    fn resolve_root_surfaces_canonicalization_failure() {
        let dir = crate::TempDir::new().unwrap();

        let error =
            resolve_root_relative_to("root", dir.path(), Some(Path::new("missing"))).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("failed to resolve root"));
    }

    #[test]
    fn find_in_ancestors_returns_match_in_the_start_directory() {
        let dir = crate::TempDir::new().unwrap();
        std::fs::write(dir.path().join("toven.toml"), b"x").unwrap();

        let found = find_in_ancestors(dir.path(), "toven.toml").unwrap();

        assert_eq!(found, dir.path().join("toven.toml"));
    }

    #[test]
    fn find_in_ancestors_walks_up_to_the_nearest_ancestor() {
        let root = crate::TempDir::new().unwrap();
        std::fs::write(root.path().join(".env"), b"x").unwrap();
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_in_ancestors(&nested, ".env").unwrap();

        assert_eq!(found, root.path().join(".env"));
    }

    #[test]
    fn find_in_ancestors_returns_none_when_absent() {
        let dir = crate::TempDir::new().unwrap();

        assert!(find_in_ancestors(dir.path(), "toven.toml").is_none());
    }

    #[test]
    fn find_in_ancestors_ignores_a_directory_with_the_target_name() {
        let dir = crate::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("toven.toml")).unwrap();

        assert!(find_in_ancestors(dir.path(), "toven.toml").is_none());
    }
}

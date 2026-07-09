//! File tree helpers.
//!
//! These helpers use blocking `std::fs` I/O. When calling them from async
//! contexts, run them through `tokio::task::spawn_blocking` or an equivalent
//! blocking executor boundary.

mod copy;
mod ignore_walk;
mod list;
mod remove;
mod types;
mod walk;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

pub use copy::copy_tree;
pub use ignore_walk::{IgnoreWalkOptions, walk_tree_ignoring};
pub use list::list_tree;
pub use remove::{remove_tree, remove_tree_if_exists};
pub use types::{CopyTreeOptions, TreeEntry, WalkControl, WalkEntryFilter, WalkOptions};
pub use walk::walk_tree;

type VisitedDirs = Option<HashSet<PathBuf>>;

fn ensure_directory(path: &Path, follow_symlinks: bool) -> AppResult<()> {
    let result = if follow_symlinks {
        std::fs::metadata(path)
    } else {
        std::fs::symlink_metadata(path)
    };
    let metadata = result.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            return AppError::new(
                ErrorCode::NotFound,
                format!("source directory not found: {}", path.display()),
            );
        }

        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to inspect source directory '{}': {error}",
                path.display()
            ),
        )
    })?;

    if !metadata.is_dir() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("source path is not a directory: {}", path.display()),
        ));
    }
    Ok(())
}

fn metadata_for(path: &Path, follow_symlinks: bool) -> AppResult<std::fs::Metadata> {
    let result = if follow_symlinks {
        std::fs::metadata(path)
    } else {
        std::fs::symlink_metadata(path)
    };
    result.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read metadata '{}': {error}", path.display()),
        )
    })
}

fn init_visited_dirs(root: &Path, follow_symlinks: bool) -> AppResult<VisitedDirs> {
    if !follow_symlinks {
        return Ok(None);
    }

    let mut visited = HashSet::new();
    visited.insert(canonical_dir(root)?);
    Ok(Some(visited))
}

fn enter_directory(path: &Path, visited: &mut VisitedDirs) -> AppResult<()> {
    let Some(visited) = visited else {
        return Ok(());
    };

    let canonical = canonical_dir(path)?;
    if !visited.insert(canonical.clone()) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("directory cycle detected at '{}'", canonical.display()),
        ));
    }
    Ok(())
}

fn canonical_dir(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize directory '{}': {error}",
                path.display()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::{canonical_dir, ensure_directory, enter_directory, metadata_for};
    use crate::TempDir;

    #[test]
    fn ensure_directory_rejects_files() {
        let dir = TempDir::new().unwrap();
        let file = dir.write_file("file.txt", b"hello").unwrap();
        let err = ensure_directory(&file, false).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_directory_rejects_symlink_roots_unless_following() {
        let dir = TempDir::new().unwrap();
        let target = dir.child("target").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.child("link").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let err = ensure_directory(&link, false).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        ensure_directory(&link, true).unwrap();
    }

    #[test]
    fn metadata_for_reports_missing_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();

        assert!(metadata_for(&missing, false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn metadata_for_reports_broken_symlink_when_following() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();
        let link = dir.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&missing, &link).unwrap();

        assert!(metadata_for(&link, true).is_err());
    }

    #[test]
    fn canonical_dir_reports_missing_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing").unwrap();

        assert!(canonical_dir(&missing).is_err());
    }

    #[test]
    fn enter_directory_rejects_cycles() {
        let dir = TempDir::new().unwrap();
        let mut visited = Some(std::collections::HashSet::new());

        enter_directory(dir.path(), &mut visited).unwrap();
        let err = enter_directory(dir.path(), &mut visited).unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn enter_directory_skips_tracking_when_disabled() {
        let dir = TempDir::new().unwrap();
        let mut visited = None;

        enter_directory(dir.path(), &mut visited).unwrap();

        assert!(visited.is_none());
    }
}

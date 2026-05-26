//! File tree helpers.

mod copy;
mod list;
mod remove;
mod types;
mod walk;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

pub use copy::copy_tree;
pub use list::list_tree;
pub use remove::{remove_tree, remove_tree_if_exists};
pub use types::{CopyTreeOptions, TreeEntry, WalkControl, WalkEntryFilter, WalkOptions};
pub use walk::walk_tree;

type VisitedDirs = HashSet<PathBuf>;

fn ensure_directory(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::new(
            ErrorCode::NotFound,
            format!("source directory not found: {}", path.display()),
        ));
    }
    if !path.is_dir() {
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

fn init_visited_dirs(root: &Path) -> AppResult<VisitedDirs> {
    let mut visited = HashSet::new();
    visited.insert(canonical_dir(root)?);
    Ok(visited)
}

fn enter_directory(path: &Path, visited: &mut VisitedDirs) -> AppResult<()> {
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

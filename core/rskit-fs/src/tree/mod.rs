//! File tree helpers.

mod copy;
mod list;
mod remove;
mod types;

pub use copy::copy_tree;
pub use list::list_tree;
pub use remove::{remove_tree, remove_tree_if_exists};
pub use types::{CopyTreeOptions, TreeEntry};

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

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

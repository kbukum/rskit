use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

pub(crate) type VisitedDirs = Option<HashSet<PathBuf>>;

pub(crate) fn ensure_directory(path: &Path, follow_symlinks: bool) -> AppResult<()> {
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

pub(crate) fn metadata_for(path: &Path, follow_symlinks: bool) -> AppResult<std::fs::Metadata> {
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

pub(crate) fn init_visited_dirs(root: &Path, follow_symlinks: bool) -> AppResult<VisitedDirs> {
    if !follow_symlinks {
        return Ok(None);
    }

    let mut visited = HashSet::new();
    visited.insert(canonical_dir(root)?);
    Ok(Some(visited))
}

pub(crate) fn enter_directory(path: &Path, visited: &mut VisitedDirs) -> AppResult<()> {
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

pub(crate) fn canonical_dir(path: &Path) -> AppResult<PathBuf> {
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

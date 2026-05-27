//! Sync directory helpers.
//!
//! These helpers use `std::fs` and may block the current thread.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::types::DirEntry;

/// Create a directory tree if it does not exist.
pub fn create_all(path: &Path) -> AppResult<()> {
    std::fs::create_dir_all(path).map_err(|error| create_dir_error(path, error))
}

/// Return true when `path` exists as a directory, without following symlinks.
pub fn exists(path: &Path) -> AppResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_dir_error(path, error)),
    }
}

/// List entries directly inside a directory.
pub fn list(path: &Path) -> AppResult<Vec<DirEntry>> {
    std::fs::read_dir(path)
        .map_err(|error| read_dir_error(path, error))?
        .map(|entry| {
            let entry = entry.map_err(read_dir_entry_error)?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_type = entry
                .file_type()
                .map_err(|error| inspect_dir_entry_error(&path, error))?;
            Ok(DirEntry {
                path,
                file_name,
                is_file: file_type.is_file(),
                is_dir: file_type.is_dir(),
                is_symlink: file_type.is_symlink(),
            })
        })
        .collect()
}

/// Return true when a directory exists and has no entries.
pub fn is_empty(path: &Path) -> AppResult<bool> {
    Ok(list(path)?.is_empty())
}

/// Remove an empty directory.
pub fn remove(path: &Path) -> AppResult<()> {
    std::fs::remove_dir(path).map_err(|error| remove_dir_error(path, error))
}

/// Remove an empty directory and ignore `NotFound`.
pub fn remove_if_exists(path: &Path) -> AppResult<bool> {
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(remove_dir_error(path, error)),
    }
}

/// Remove a directory tree recursively.
pub fn remove_all(path: &Path) -> AppResult<()> {
    std::fs::remove_dir_all(path).map_err(|error| remove_tree_error(path, error))
}

/// Remove a directory tree recursively and ignore `NotFound`.
pub fn remove_all_if_exists(path: &Path) -> AppResult<bool> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(remove_tree_error(path, error)),
    }
}

fn create_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create directory '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn inspect_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to inspect directory '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn read_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn read_dir_entry_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory entry: {error}"),
    )
    .with_cause(error)
}

fn inspect_dir_entry_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to inspect directory entry '{}': {error}",
            path.display()
        ),
    )
    .with_cause(error)
}

fn remove_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to remove directory '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn remove_tree_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to remove directory tree '{}': {error}",
            path.display()
        ),
    )
    .with_cause(error)
}

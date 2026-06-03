//! Shared file error classification helpers.

use std::path::Path;

use rskit_errors::{AppError, ErrorCode};

const RSKIT_FS_ERROR: &str = "rskit_fs_error";
const FILE_TOO_LARGE: &str = "file_too_large";
const NOT_REGULAR_FILE: &str = "not_regular_file";
const SYMLINK_NOT_ALLOWED: &str = "symlink_not_allowed";

/// Return true when `error` was created by bounded file-read size enforcement.
#[must_use]
pub fn is_file_too_large_error(error: &AppError) -> bool {
    has_fs_error(error, FILE_TOO_LARGE)
}

/// Return true when `error` was created by no-follow symlink rejection.
#[must_use]
pub fn is_symlink_not_allowed_error(error: &AppError) -> bool {
    has_fs_error(error, SYMLINK_NOT_ALLOWED)
}

/// Return true when `error` was created by regular-file validation.
#[must_use]
pub fn is_not_regular_file_error(error: &AppError) -> bool {
    has_fs_error(error, NOT_REGULAR_FILE)
}

fn has_fs_error(error: &AppError, kind: &str) -> bool {
    error
        .details()
        .get(RSKIT_FS_ERROR)
        .and_then(|value| value.as_str())
        == Some(kind)
}

pub(crate) fn file_too_large_error(path: &Path, actual: u64, limit: u64) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "file '{}' is {actual} bytes, exceeding limit {limit} bytes",
            path.display()
        ),
    )
    .with_detail(RSKIT_FS_ERROR, FILE_TOO_LARGE)
}

pub(crate) fn not_regular_file_error(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("path is not a regular file: {}", path.display()),
    )
    .with_detail(RSKIT_FS_ERROR, NOT_REGULAR_FILE)
}

pub(crate) fn symlink_not_allowed_error(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("symlinks are not allowed: {}", path.display()),
    )
    .with_detail(RSKIT_FS_ERROR, SYMLINK_NOT_ALLOWED)
}

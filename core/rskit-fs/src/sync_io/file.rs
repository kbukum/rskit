//! Sync file helpers.
//!
//! These helpers use `std::fs` and may block the current thread.

use std::fs::{File, OpenOptions};
use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::types::FileMeta;

use crate::path::parent_dir;
use crate::temp::sibling_temp_path;

const RSKIT_FS_ERROR: &str = "rskit_fs_error";
const FILE_TOO_LARGE: &str = "file_too_large";
const NOT_REGULAR_FILE: &str = "not_regular_file";
const SYMLINK_NOT_ALLOWED: &str = "symlink_not_allowed";
const WRITE_ATOMIC_TEMP_ATTEMPTS: usize = 16;

/// Create the parent directory for a file path if it has one.
pub fn create_parent_dir(path: &Path) -> AppResult<()> {
    if let Some(parent) = parent_dir(path) {
        std::fs::create_dir_all(parent).map_err(create_parent_dirs_error)?;
    }
    Ok(())
}

/// Open a file for blocking reads.
pub fn open(path: &Path) -> AppResult<File> {
    File::open(path).map_err(|error| open_file_error(path, error))
}

/// Create a file for blocking writes, creating parent directories as needed.
pub fn create(path: &Path) -> AppResult<File> {
    create_parent_dir(path)?;
    File::create(path).map_err(|error| create_file_error(path, error))
}

/// Return true when `path` exists as a regular file, without following symlinks.
pub fn exists(path: &Path) -> AppResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_file_error(path, error)),
    }
}

/// Open a regular file without following a final symlink.
pub fn open_no_follow_regular(path: &Path) -> AppResult<File> {
    let file = open_no_follow(path)?;
    let metadata = file
        .metadata()
        .map_err(|error| inspect_file_error(path, error))?;
    if !metadata.is_file() {
        return Err(not_regular_file_error(path));
    }
    Ok(file)
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> AppResult<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| open_file_error(path, error))
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> AppResult<File> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| inspect_file_error(path, error))?;
    if metadata.file_type().is_symlink() {
        return Err(
            symlink_not_allowed_error(path).with_cause(std::io::Error::other("path is a symlink"))
        );
    }
    open(path)
}

/// Read a file into memory.
pub fn read(path: &Path) -> AppResult<Vec<u8>> {
    std::fs::read(path).map_err(|error| read_file_error(path, error))
}

/// Read a UTF-8 text file.
pub fn read_string(path: &Path) -> AppResult<String> {
    std::fs::read_to_string(path).map_err(|error| read_file_error(path, error))
}

/// Read at most `max_bytes` from a regular file.
pub fn read_bounded(path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut file = open_no_follow_regular(path)?;
    read_bounded_from_file(path, max_bytes, &mut file)
}

/// Read a UTF-8 text file up to `max_bytes`.
pub fn read_string_bounded(path: &Path, max_bytes: u64) -> AppResult<String> {
    let bytes = read_bounded(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("file '{}' is not valid UTF-8: {error}", path.display()),
        )
    })
}

/// Read at most `max_bytes` bytes from a regular file without following a final symlink.
pub fn read_no_follow_regular_bounded(path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut file = open_no_follow_regular(path)?;
    read_bounded_from_file(path, max_bytes, &mut file)
}

fn read_bounded_from_file(path: &Path, max_bytes: u64, file: &mut File) -> AppResult<Vec<u8>> {
    let metadata = file
        .metadata()
        .map_err(|error| inspect_file_error(path, error))?;
    if metadata.is_file() && metadata.len() > max_bytes {
        return Err(file_too_large_error(path, metadata.len(), max_bytes));
    }

    let capacity = metadata
        .len()
        .min(max_bytes)
        .try_into()
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| read_file_error(path, error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(file_too_large_error(path, bytes.len() as u64, max_bytes));
    }
    Ok(bytes)
}

/// Write bytes to a file, creating parent directories as needed.
pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> AppResult<()> {
    create_parent_dir(path)?;
    std::fs::write(path, bytes).map_err(|error| write_file_error(path, error))
}

/// Copy one file to another path, creating parent directories as needed.
pub fn copy(from: &Path, to: &Path) -> AppResult<u64> {
    create_parent_dir(to)?;
    std::fs::copy(from, to).map_err(|error| copy_file_error(from, to, error))
}

/// Rename or move a file, creating the destination parent directory as needed.
pub fn rename(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to)?;
    std::fs::rename(from, to).map_err(|error| rename_file_error(from, to, error))
}

/// Move a file, falling back to copy+delete when rename cannot cross filesystems.
pub fn move_file(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to)?;
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if is_cross_device_error(&error) => {
            copy(from, to)?;
            remove(from)
        }
        Err(error) => Err(move_file_error(from, to, error)),
    }
}

/// Remove a file.
pub fn remove(path: &Path) -> AppResult<()> {
    std::fs::remove_file(path).map_err(|error| remove_file_error(path, error))
}

/// Remove a file and ignore `NotFound`.
pub fn remove_if_exists(path: &Path) -> AppResult<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(remove_file_error(path, error)),
    }
}

/// Read file metadata without following symlinks.
pub fn metadata(path: &Path) -> AppResult<FileMeta> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| inspect_file_error(path, error))?;
    Ok(FileMeta {
        path: path.to_path_buf(),
        len: metadata.len(),
        created: metadata.created().ok(),
        modified: metadata.modified().ok(),
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        is_symlink: metadata.file_type().is_symlink(),
    })
}

/// Atomically write bytes by writing a sibling temp file and renaming it.
pub fn write_atomic(dest: &Path, bytes: impl AsRef<[u8]>, temp_prefix: &str) -> AppResult<()> {
    write_atomic_with_attempts(dest, bytes, temp_prefix, WRITE_ATOMIC_TEMP_ATTEMPTS)
}

fn write_atomic_with_attempts(
    dest: &Path,
    bytes: impl AsRef<[u8]>,
    temp_prefix: &str,
    attempts: usize,
) -> AppResult<()> {
    create_parent_dir(dest)?;
    let bytes = bytes.as_ref();

    for _ in 0..attempts {
        let temp_path = sibling_temp_path(dest, temp_prefix, ".tmp");
        let mut temp_file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(create_file_error(&temp_path, error)),
        };

        let result = (|| {
            use std::io::Write as _;
            temp_file
                .write_all(bytes)
                .map_err(|error| write_file_error(&temp_path, error))?;
            temp_file
                .sync_data()
                .map_err(|error| sync_file_error(&temp_path, error))?;
            drop(temp_file);
            rename(&temp_path, dest)
        })();

        if result.is_err() {
            let _ = remove_if_exists(&temp_path);
        }
        return result;
    }

    Err(AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to create a unique temp file for '{}' after {attempts} attempts",
            dest.display()
        ),
    ))
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

fn is_cross_device_error(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::EXDEV)
    }
    #[cfg(not(unix))]
    {
        error.kind() == std::io::ErrorKind::CrossesDevices
    }
}

fn create_parent_dirs_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create parent dirs: {error}"),
    )
    .with_cause(error)
}

fn inspect_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to inspect file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn open_file_error(path: &Path, error: std::io::Error) -> AppError {
    if is_symlink_open_error(&error) {
        return symlink_not_allowed_error(path).with_cause(error);
    }

    AppError::new(
        ErrorCode::Internal,
        format!("failed to open file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn is_symlink_open_error(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ELOOP)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn create_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn read_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn write_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to write file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn copy_file_error(from: &Path, to: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to copy '{}' to '{}': {error}",
            from.display(),
            to.display()
        ),
    )
    .with_cause(error)
}

fn rename_file_error(from: &Path, to: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to rename '{}' to '{}': {error}",
            from.display(),
            to.display()
        ),
    )
    .with_cause(error)
}

fn move_file_error(from: &Path, to: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to move '{}' to '{}': {error}",
            from.display(),
            to.display()
        ),
    )
    .with_cause(error)
}

fn remove_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to remove '{}': {error}", path.display()),
    )
    .with_cause(error)
}

fn sync_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to sync file '{}': {error}", path.display()),
    )
    .with_cause(error)
}

/// Return true when `error` was created by bounded file-read size enforcement.
pub fn is_file_too_large_error(error: &AppError) -> bool {
    has_fs_error(error, FILE_TOO_LARGE)
}

/// Return true when `error` was created by no-follow symlink rejection.
pub fn is_symlink_not_allowed_error(error: &AppError) -> bool {
    has_fs_error(error, SYMLINK_NOT_ALLOWED)
}

/// Return true when `error` was created by regular-file validation.
pub fn is_not_regular_file_error(error: &AppError) -> bool {
    has_fs_error(error, NOT_REGULAR_FILE)
}

fn has_fs_error(error: &AppError, kind: &str) -> bool {
    error
        .details
        .get(RSKIT_FS_ERROR)
        .and_then(|value| value.as_str())
        == Some(kind)
}

fn file_too_large_error(path: &Path, actual: u64, limit: u64) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "file '{}' is {actual} bytes, exceeding limit {limit} bytes",
            path.display()
        ),
    )
    .with_detail(RSKIT_FS_ERROR, FILE_TOO_LARGE)
}

fn not_regular_file_error(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("path is not a regular file: {}", path.display()),
    )
    .with_detail(RSKIT_FS_ERROR, NOT_REGULAR_FILE)
}

fn symlink_not_allowed_error(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("symlinks are not allowed: {}", path.display()),
    )
    .with_detail(RSKIT_FS_ERROR, SYMLINK_NOT_ALLOWED)
}

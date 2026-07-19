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

use crate::file_error::{file_too_large_error, not_regular_file_error, symlink_not_allowed_error};
pub use crate::file_error::{
    is_file_too_large_error, is_not_regular_file_error, is_symlink_not_allowed_error,
};
use crate::path::parent_dir;
use crate::temp::sibling_temp_path;

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

/// Return true when `path` resolves to a regular file the OS marks executable.
///
/// Follows symlinks (so a symlinked launcher on `PATH` resolves to its target), and reports `false` —
/// rather than erroring — for a missing path,
/// so callers scanning a search path can simply skip non-executable candidates. On Unix,
/// "executable" means any of the owner/group/other execute bits is set;
/// on other platforms the concept is not modeled in the same way,
/// so any regular file is treated as executable.
pub fn is_executable(path: &Path) -> AppResult<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && metadata_has_exec_bit(&metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_file_error(path, error)),
    }
}

#[cfg(unix)]
fn metadata_has_exec_bit(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn metadata_has_exec_bit(_metadata: &std::fs::Metadata) -> bool {
    true
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

/// Read at most `max_bytes` from a regular file without following a final symlink.
pub fn read_bounded(path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut file = open_no_follow_regular(path)?;
    read_bounded_from_file(path, max_bytes, &mut file)
}

/// Read a UTF-8 text file up to `max_bytes` bytes without following a final symlink.
pub fn read_string_bounded(path: &Path, max_bytes: u64) -> AppResult<String> {
    let bytes = read_bounded(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("file '{}' is not valid UTF-8: {error}", path.display()),
        )
    })
}

fn read_bounded_from_file(path: &Path, max_bytes: u64, file: &mut File) -> AppResult<Vec<u8>> {
    let metadata = file
        .metadata()
        .map_err(|error| inspect_file_error(path, error))?;
    if metadata.is_file() && metadata.len() > max_bytes {
        return Err(file_too_large_error(path, metadata.len(), max_bytes));
    }

    let capacity = metadata.len().min(max_bytes).try_into().unwrap_or(0);
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
    write_atomic_with_attempts(dest, bytes, temp_prefix, WRITE_ATOMIC_TEMP_ATTEMPTS, false)
}

/// Atomically write bytes and replace an existing destination when supported.
///
/// Replacing an existing destination is atomic on Unix-like platforms. On Windows,
/// this helper removes the existing file before persisting the temp file because the platform rename operation cannot replace an existing file.
pub fn write_atomic_replace(
    dest: &Path,
    bytes: impl AsRef<[u8]>,
    temp_prefix: &str,
) -> AppResult<()> {
    write_atomic_with_attempts(dest, bytes, temp_prefix, WRITE_ATOMIC_TEMP_ATTEMPTS, true)
}

fn write_atomic_with_attempts(
    dest: &Path,
    bytes: impl AsRef<[u8]>,
    temp_prefix: &str,
    attempts: usize,
    replace_existing: bool,
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
            persist_temp_file_with_replace(&temp_path, dest, replace_existing)
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

fn persist_temp_file_with_replace(
    temp_path: &Path,
    dest: &Path,
    replace_existing: bool,
) -> AppResult<()> {
    #[cfg(windows)]
    if replace_existing {
        remove_if_exists(dest)?;
    }

    let _ = replace_existing;
    rename(temp_path, dest)
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

#[cfg(test)]
mod tests {
    use super::{
        canonicalize, copy, create, create_file_error, create_parent_dir, create_parent_dirs_error,
        inspect_file_error, is_cross_device_error, is_executable, is_file_too_large_error,
        is_not_regular_file_error, is_symlink_not_allowed_error, move_file_error, open_file_error,
        persist_temp_file_with_replace, read_bounded, read_file_error, read_string,
        read_string_bounded, remove, remove_file_error, remove_if_exists, rename_file_error,
        sync_file_error, write, write_atomic, write_atomic_replace, write_file_error,
    };

    use crate::TempDir;
    use rskit_errors::ErrorCode;
    use std::io;
    use std::path::Path;

    #[test]
    fn is_executable_is_false_for_a_missing_path() {
        let root = TempDir::new().unwrap();
        let missing = root.child("missing").unwrap();

        assert!(!is_executable(&missing).unwrap());
    }

    #[test]
    fn is_executable_is_false_for_a_directory() {
        let root = TempDir::new().unwrap();

        assert!(!is_executable(root.path()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn is_executable_tracks_the_unix_execute_bits() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = TempDir::new().unwrap();
        let path = root.write_file("tool", b"#!/bin/sh\n").unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable(&path).unwrap(), "non-executable file");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&path).unwrap(), "executable file");
    }

    #[cfg(not(unix))]
    #[test]
    fn is_executable_accepts_any_regular_file_off_unix() {
        let root = TempDir::new().unwrap();
        let path = root.write_file("tool", b"binary").unwrap();

        assert!(is_executable(&path).unwrap());
    }

    #[test]
    fn bounded_read_accepts_regular_files_within_limit() {
        let root = TempDir::new().unwrap();
        let path = root.write_file("file.txt", b"hello").unwrap();

        assert_eq!(read_bounded(&path, 5).unwrap(), b"hello");
        assert_eq!(read_string_bounded(&path, 5).unwrap(), "hello");
    }

    #[test]
    fn bounded_read_rejects_oversized_files() {
        let root = TempDir::new().unwrap();
        let path = root.write_file("file.txt", b"hello").unwrap();

        let error = read_bounded(&path, 4).unwrap_err();

        assert!(is_file_too_large_error(&error));
    }

    #[test]
    fn bounded_read_rejects_directories() {
        let root = TempDir::new().unwrap();

        let error = read_bounded(root.path(), 1024).unwrap_err();

        assert!(is_not_regular_file_error(&error));
    }

    #[cfg(unix)]
    #[test]
    fn bounded_read_rejects_final_symlinks() {
        let root = TempDir::new().unwrap();
        let target = root.write_file("target.txt", b"hello").unwrap();
        let link = root.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let error = read_bounded(&link, 1024).unwrap_err();

        assert!(is_symlink_not_allowed_error(&error));
    }

    #[test]
    fn atomic_replace_overwrites_existing_files() {
        let root = TempDir::new().unwrap();
        let path = root.write_file("file.txt", b"old").unwrap();

        write_atomic_replace(&path, b"new", "test").unwrap();

        assert_eq!(read_string(&path).unwrap(), "new");
    }

    #[test]
    fn replace_policy_still_rejects_destination_directories() {
        let root = TempDir::new().unwrap();
        let temp = root.write_file("temp.txt", b"temp").unwrap();
        let dest = root.child("dest").unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        assert!(persist_temp_file_with_replace(&temp, &dest, true).is_err());
    }

    #[test]
    fn file_helpers_cover_regular_success_and_missing_paths() {
        let root = TempDir::new().unwrap();
        let path = root.child("nested/file.txt").unwrap();
        create_parent_dir(&path).unwrap();
        write(&path, b"hello").unwrap();
        assert_eq!(read_string(&path).unwrap(), "hello");
        let copy_path = root.child("copy.txt").unwrap();
        assert_eq!(copy(&path, &copy_path).unwrap(), 5);
        let moved = root.child("moved.txt").unwrap();
        super::rename(&copy_path, &moved).unwrap();
        assert!(super::metadata(&moved).unwrap().is_file);
        assert!(remove_if_exists(&moved).unwrap());
        assert!(!remove_if_exists(&moved).unwrap());
        assert!(canonicalize(&path).unwrap().is_absolute());

        let created = root.child("created.txt").unwrap();
        drop(create(&created).unwrap());
        write_atomic(&created, b"atomic", "test").unwrap();
        assert_eq!(read_string(&created).unwrap(), "atomic");
        remove(&created).unwrap();
    }

    #[test]
    fn file_error_helpers_preserve_context() {
        let path = Path::new("file");
        let other = || io::Error::other("boom");
        let errors = [
            create_parent_dirs_error(other()),
            inspect_file_error(path, other()),
            open_file_error(path, other()),
            create_file_error(path, other()),
            read_file_error(path, other()),
            write_file_error(path, other()),
            super::copy_file_error(path, Path::new("to"), other()),
            rename_file_error(path, Path::new("to"), other()),
            move_file_error(path, Path::new("to"), other()),
            remove_file_error(path, other()),
            sync_file_error(path, other()),
        ];

        for error in errors {
            assert_eq!(error.code(), ErrorCode::Internal);
            assert!(error.cause().is_some());
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_open_and_cross_device_errors_are_detected() {
        let symlink_error = io::Error::from_raw_os_error(libc::ELOOP);
        assert!(is_symlink_not_allowed_error(&open_file_error(
            Path::new("link"),
            symlink_error
        )));
        assert!(is_cross_device_error(&io::Error::from_raw_os_error(
            libc::EXDEV
        )));
    }
}

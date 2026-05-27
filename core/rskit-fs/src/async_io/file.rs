//! Async file helpers.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::path::parent_dir;
use crate::temp::sibling_temp_path;
use crate::types::FileMeta;

const WRITE_ATOMIC_TEMP_ATTEMPTS: usize = 16;

/// Async file handle opened through this crate.
pub type AsyncFile = tokio::fs::File;

/// Create the parent directory for a file path if it has one.
pub async fn create_parent_dir(path: &Path) -> AppResult<()> {
    if let Some(parent) = parent_dir(path) {
        super::dir::create_all(parent).await?;
    }
    Ok(())
}

/// Return true when `path` exists as a regular file, without following symlinks.
pub async fn exists(path: &Path) -> AppResult<bool> {
    exists_from_metadata(path, tokio::fs::symlink_metadata(path).await)
}

fn exists_from_metadata(
    path: &Path,
    result: std::io::Result<std::fs::Metadata>,
) -> AppResult<bool> {
    match result {
        Ok(metadata) => Ok(metadata.is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_file_error(path, error)),
    }
}

/// Read file metadata without following symlinks.
pub async fn metadata(path: &Path) -> AppResult<FileMeta> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|error| inspect_file_error(path, error))?;
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

/// Read a file into memory.
pub async fn read(path: &Path) -> AppResult<Vec<u8>> {
    tokio::fs::read(path)
        .await
        .map_err(|error| read_file_error(path, error))
}

/// Read a UTF-8 text file.
pub async fn read_string(path: &Path) -> AppResult<String> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| read_file_error(path, error))
}

/// Read at most `max_bytes` bytes from a file.
pub async fn read_bounded(path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    let file = open(path).await?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| inspect_file_error(path, error))?;
    if metadata.is_file() && metadata.len() > max_bytes {
        return Err(file_too_large_error(path, metadata.len(), max_bytes));
    }

    let capacity = metadata.len().min(max_bytes).try_into().unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| read_file_error(path, error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(file_too_large_error(path, bytes.len() as u64, max_bytes));
    }
    Ok(bytes)
}

/// Read a UTF-8 text file up to `max_bytes` bytes.
pub async fn read_string_bounded(path: &Path, max_bytes: u64) -> AppResult<String> {
    let bytes = read_bounded(path, max_bytes).await?;
    String::from_utf8(bytes).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("file '{}' is not valid UTF-8: {error}", path.display()),
        )
    })
}

/// Write bytes to a file, creating parent directories as needed.
pub async fn write(path: &Path, bytes: impl AsRef<[u8]>) -> AppResult<()> {
    create_parent_dir(path).await?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(|error| write_file_error(path, error))
}

/// Open a file for async reading.
pub async fn open(path: &Path) -> AppResult<AsyncFile> {
    tokio::fs::File::open(path)
        .await
        .map_err(|error| open_file_error(path, error))
}

/// Create a file for async writing, creating parent directories as needed.
pub async fn create(path: &Path) -> AppResult<AsyncFile> {
    create_parent_dir(path).await?;
    tokio::fs::File::create(path)
        .await
        .map_err(|error| create_file_error(path, error))
}

/// Persist a temp file to `dest` using the platform rename operation.
///
/// Replacing an existing destination is atomic on Unix-like platforms. On
/// Windows, replacing an existing destination is not supported by this helper
/// because `rename` fails when `dest` already exists.
pub async fn persist_temp_file(temp_path: &Path, dest: &Path) -> AppResult<()> {
    tokio::fs::rename(temp_path, dest)
        .await
        .map_err(|error| rename_file_error(temp_path, dest, error))
}

/// Copy one file to another path, creating parent directories as needed.
pub async fn copy(from: &Path, to: &Path) -> AppResult<u64> {
    create_parent_dir(to).await?;
    tokio::fs::copy(from, to)
        .await
        .map_err(|error| copy_file_error(from, to, error))
}

/// Rename or move a file, creating the destination parent directory as needed.
pub async fn rename(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to).await?;
    tokio::fs::rename(from, to)
        .await
        .map_err(|error| rename_file_error(from, to, error))
}

/// Move a file, falling back to copy+delete when a platform rename cannot cross filesystems.
///
/// This fallback is not atomic across filesystems. Use [`rename`] when atomic
/// same-filesystem replacement is required.
pub async fn move_file(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to).await?;
    move_file_after_rename(from, to, tokio::fs::rename(from, to).await).await
}

async fn move_file_after_rename(
    from: &Path,
    to: &Path,
    result: std::io::Result<()>,
) -> AppResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if is_cross_device_error(&error) => {
            copy(from, to).await?;
            remove(from).await
        }
        Err(error) => Err(move_file_error(from, to, error)),
    }
}

/// Remove a file.
pub async fn remove(path: &Path) -> AppResult<()> {
    tokio::fs::remove_file(path)
        .await
        .map_err(|error| remove_file_error(path, error))
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

/// Remove a file and ignore `NotFound`.
pub async fn remove_if_exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(remove_file_error(path, error)),
    }
}

/// Atomically write bytes by writing a sibling temp file and renaming it.
///
/// Replacing an existing destination is atomic on Unix-like platforms. On
/// Windows, this helper succeeds for new destinations and returns an error when
/// replacing an existing destination.
pub async fn write_atomic(
    dest: &Path,
    bytes: impl AsRef<[u8]>,
    temp_prefix: &str,
) -> AppResult<()> {
    write_atomic_with_attempts(dest, bytes, temp_prefix, WRITE_ATOMIC_TEMP_ATTEMPTS).await
}

async fn write_atomic_with_attempts(
    dest: &Path,
    bytes: impl AsRef<[u8]>,
    temp_prefix: &str,
    attempts: usize,
) -> AppResult<()> {
    create_parent_dir(dest).await?;

    let bytes = bytes.as_ref();
    for _ in 0..attempts {
        let temp_path = sibling_temp_path(dest, temp_prefix, ".tmp");
        let mut temp_file = match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await
        {
            Ok(file) => file,
            Err(error) if should_retry_temp_open(&error) => continue,
            Err(error) => return Err(create_temp_file_error(&temp_path, error)),
        };

        let result = async {
            temp_file
                .write_all(bytes)
                .await
                .map_err(|error| write_temp_file_error(&temp_path, error))?;
            temp_file
                .sync_data()
                .await
                .map_err(|error| sync_temp_file_error(&temp_path, error))?;
            drop(temp_file);
            persist_temp_file(&temp_path, dest).await
        }
        .await;

        if result.is_err() {
            let _ = remove_if_exists(&temp_path).await;
        }

        return result;
    }

    Err(unique_temp_file_error(dest, attempts))
}

fn should_retry_temp_open(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::AlreadyExists
}

fn inspect_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to inspect file '{}': {error}", path.display()),
    )
}

fn read_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read file '{}': {error}", path.display()),
    )
}

fn open_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to open file '{}': {error}", path.display()),
    )
}

fn create_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create file '{}': {error}", path.display()),
    )
}

fn write_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to write file '{}': {error}", path.display()),
    )
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
}

fn remove_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to remove '{}': {error}", path.display()),
    )
}

fn create_temp_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create temp file '{}': {error}", path.display()),
    )
}

fn write_temp_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to write temp file '{}': {error}", path.display()),
    )
}

fn sync_temp_file_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to sync temp file '{}': {error}", path.display()),
    )
}

fn unique_temp_file_error(dest: &Path, attempts: usize) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to create a unique temp file for '{}' after {attempts} attempts",
            dest.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        WRITE_ATOMIC_TEMP_ATTEMPTS, copy, copy_file_error, create_parent_dir,
        create_temp_file_error, exists, exists_from_metadata, inspect_file_error, metadata,
        move_file, move_file_after_rename, move_file_error, persist_temp_file, read,
        read_file_error, read_string, remove, remove_file_error, remove_if_exists, rename,
        rename_file_error, should_retry_temp_open, sync_temp_file_error, unique_temp_file_error,
        write, write_atomic, write_atomic_with_attempts, write_file_error, write_temp_file_error,
    };
    use crate::TempDir;

    #[tokio::test]
    async fn file_lifecycle() {
        let root = TempDir::new().unwrap();
        let path = root.child("a/b.txt").unwrap();

        write(&path, b"hello").await.unwrap();
        assert!(exists(&path).await.unwrap());
        assert_eq!(read(&path).await.unwrap(), b"hello");
        assert_eq!(read_string(&path).await.unwrap(), "hello");
        assert_eq!(metadata(&path).await.unwrap().len, 5);

        let copy_path = root.child("copy/b.txt").unwrap();
        assert_eq!(copy(&path, &copy_path).await.unwrap(), 5);
        assert_eq!(read_string(&copy_path).await.unwrap(), "hello");

        let renamed = root.child("renamed/b.txt").unwrap();
        rename(&copy_path, &renamed).await.unwrap();
        assert!(!exists(&copy_path).await.unwrap());
        assert!(exists(&renamed).await.unwrap());
        assert!(remove_if_exists(&renamed).await.unwrap());
        assert!(!remove_if_exists(&renamed).await.unwrap());
    }

    #[tokio::test]
    async fn create_parent_dir_ignores_paths_without_parent() {
        create_parent_dir(std::path::Path::new("file.txt"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn file_error_paths_are_reported() {
        let root = TempDir::new().unwrap();
        let missing = root.child("missing.txt").unwrap();
        let dir = root.child("dir").unwrap();
        crate::async_io::dir::create_all(&dir).await.unwrap();
        let nested_under_file = root.child("file.txt/child.txt").unwrap();
        root.write_file("file.txt", b"hello").unwrap();

        assert!(!exists(&missing).await.unwrap());
        assert!(read(&missing).await.is_err());
        assert!(read_string(&missing).await.is_err());
        assert!(write(&nested_under_file, b"nope").await.is_err());
        assert!(
            copy(&missing, &root.child("copy.txt").unwrap())
                .await
                .is_err()
        );
        assert!(
            rename(&missing, &root.child("renamed.txt").unwrap())
                .await
                .is_err()
        );
        assert!(remove(&missing).await.is_err());
        assert!(remove_if_exists(&dir).await.is_err());
    }

    #[test]
    fn file_error_builders_include_context() {
        let from = std::path::Path::new("from.txt");
        let to = std::path::Path::new("to.txt");
        let err = || std::io::Error::other("boom");

        assert!(
            inspect_file_error(from, err())
                .to_string()
                .contains("inspect file")
        );
        assert!(
            read_file_error(from, err())
                .to_string()
                .contains("read file")
        );
        assert!(
            write_file_error(from, err())
                .to_string()
                .contains("write file")
        );
        assert!(
            copy_file_error(from, to, err())
                .to_string()
                .contains("copy")
        );
        assert!(
            rename_file_error(from, to, err())
                .to_string()
                .contains("rename")
        );
        assert!(
            move_file_error(from, to, err())
                .to_string()
                .contains("move")
        );
        assert!(
            remove_file_error(from, err())
                .to_string()
                .contains("remove")
        );
        assert!(
            create_temp_file_error(from, err())
                .to_string()
                .contains("create temp file")
        );
        assert!(
            write_temp_file_error(from, err())
                .to_string()
                .contains("write temp file")
        );
        assert!(
            sync_temp_file_error(from, err())
                .to_string()
                .contains("sync temp file")
        );
        assert!(
            unique_temp_file_error(to, WRITE_ATOMIC_TEMP_ATTEMPTS)
                .to_string()
                .contains("unique temp file")
        );
        assert!(exists_from_metadata(from, Err(err())).is_err());
        assert!(should_retry_temp_open(&std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exists",
        )));
        assert!(!should_retry_temp_open(&err()));
    }

    #[tokio::test]
    async fn metadata_reports_symlinks_and_missing_errors() {
        let root = TempDir::new().unwrap();
        let missing = root.child("missing.txt").unwrap();
        assert!(metadata(&missing).await.is_err());

        #[cfg(unix)]
        {
            let file = root.write_file("file.txt", b"hello").unwrap();
            let link = root.child("link.txt").unwrap();
            std::os::unix::fs::symlink(&file, &link).unwrap();
            let meta = metadata(&link).await.unwrap();
            assert_eq!(meta.path, link);
            assert!(meta.is_symlink);
            assert!(meta.modified.is_some());
        }
    }

    #[tokio::test]
    async fn persist_and_move_helpers_cover_success_and_errors() {
        let root = TempDir::new().unwrap();
        let temp = root.write_file("temp.txt", b"temp").unwrap();
        let dest = root.child("dest.txt").unwrap();
        persist_temp_file(&temp, &dest).await.unwrap();
        assert_eq!(read_string(&dest).await.unwrap(), "temp");

        let missing = root.child("missing.txt").unwrap();
        assert!(
            persist_temp_file(&missing, &root.child("other.txt").unwrap())
                .await
                .is_err()
        );

        let moved = root.child("moved.txt").unwrap();
        move_file(&dest, &moved).await.unwrap();
        assert_eq!(read_string(&moved).await.unwrap(), "temp");
        assert!(
            move_file(&missing, &root.child("nope.txt").unwrap())
                .await
                .is_err()
        );

        #[cfg(unix)]
        {
            let source = root.write_file("cross-device.txt", b"temp").unwrap();
            let target = root.child("cross-device-moved.txt").unwrap();
            move_file_after_rename(
                &source,
                &target,
                Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
            )
            .await
            .unwrap();
            assert_eq!(read_string(&target).await.unwrap(), "temp");
            assert!(!source.exists());
        }
    }

    #[tokio::test]
    async fn atomic_write_creates_parent_dirs() {
        let root = TempDir::new().unwrap();
        let path = root.child("nested/file.txt").unwrap();

        write_atomic(&path, b"atomic", "test").await.unwrap();

        assert_eq!(read_string(&path).await.unwrap(), "atomic");
    }

    #[tokio::test]
    async fn atomic_write_sanitizes_temp_prefix() {
        let root = TempDir::new().unwrap();
        let path = root.child("nested/file.txt").unwrap();

        write_atomic(&path, b"atomic", "../escape").await.unwrap();

        assert_eq!(read_string(&path).await.unwrap(), "atomic");
        assert!(!root.child("escape").unwrap().exists());
    }

    #[tokio::test]
    async fn atomic_write_reports_destination_parent_errors() {
        let root = TempDir::new().unwrap();
        root.write_file("file.txt", b"hello").unwrap();
        let path = root.child("file.txt/nested.txt").unwrap();

        assert!(write_atomic(&path, b"atomic", "test").await.is_err());
    }

    #[tokio::test]
    async fn atomic_write_reports_persist_and_attempt_errors() {
        let root = TempDir::new().unwrap();
        let dest_dir = root.child("dest").unwrap();
        crate::async_io::dir::create_all(&dest_dir).await.unwrap();

        assert!(write_atomic(&dest_dir, b"atomic", "test").await.is_err());
        assert!(
            write_atomic_with_attempts(&root.child("file.txt").unwrap(), b"atomic", "test", 0)
                .await
                .is_err()
        );
        assert!(
            write_atomic(
                &root.child("too-long.txt").unwrap(),
                b"atomic",
                &"x".repeat(300)
            )
            .await
            .is_err()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exists_rejects_symlinks_to_files() {
        let root = TempDir::new().unwrap();
        let path = root.child("file.txt").unwrap();
        let link = root.child("link.txt").unwrap();
        write(&path, b"hello").await.unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();

        assert!(!exists(&link).await.unwrap());
    }
}

/// Return true when `error` was created by bounded file-read size enforcement.
pub fn is_file_too_large_error(error: &AppError) -> bool {
    error
        .details
        .get("rskit_fs_error")
        .and_then(|value| value.as_str())
        == Some("file_too_large")
}

fn file_too_large_error(path: &Path, actual: u64, limit: u64) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "file '{}' is {actual} bytes, exceeding limit {limit} bytes",
            path.display()
        ),
    )
    .with_detail("rskit_fs_error", "file_too_large")
}

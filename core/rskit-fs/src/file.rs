//! File helpers.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::dir::create_dir_all;
use crate::path::parent_dir;
use crate::temp::sibling_temp_path;

/// Metadata for a filesystem entry at a file path.
#[derive(Debug, Clone)]
pub struct FileMeta {
    /// File path.
    pub path: PathBuf,
    /// File size in bytes.
    pub len: u64,
    /// Last modification time, when available.
    pub modified: Option<std::time::SystemTime>,
    /// Whether this path is a symlink.
    pub is_symlink: bool,
}

/// Create the parent directory for a file path if it has one.
pub async fn create_parent_dir(path: &Path) -> AppResult<()> {
    if let Some(parent) = parent_dir(path) {
        create_dir_all(parent).await?;
    }
    Ok(())
}

/// Return true when `path` exists as a regular file, without following symlinks.
pub async fn exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => Ok(metadata.is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to inspect file '{}': {error}", path.display()),
        )),
    }
}

/// Read file metadata without following symlinks.
pub async fn metadata(path: &Path) -> AppResult<FileMeta> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to inspect file '{}': {error}", path.display()),
        )
    })?;
    Ok(FileMeta {
        path: path.to_path_buf(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
        is_symlink: metadata.file_type().is_symlink(),
    })
}

/// Read a file into memory.
pub async fn read(path: &Path) -> AppResult<Vec<u8>> {
    tokio::fs::read(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read file '{}': {error}", path.display()),
        )
    })
}

/// Read a UTF-8 text file.
pub async fn read_string(path: &Path) -> AppResult<String> {
    tokio::fs::read_to_string(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read file '{}': {error}", path.display()),
        )
    })
}

/// Write bytes to a file, creating parent directories as needed.
pub async fn write(path: &Path, bytes: impl AsRef<[u8]>) -> AppResult<()> {
    create_parent_dir(path).await?;
    tokio::fs::write(path, bytes).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to write file '{}': {error}", path.display()),
        )
    })
}

/// Persist a temp file to `dest` using the platform rename operation.
///
/// Replacing an existing destination is atomic on Unix-like platforms. On
/// Windows, replacing an existing destination is not supported by this helper
/// because `rename` fails when `dest` already exists.
pub async fn persist_temp_file(temp_path: &Path, dest: &Path) -> AppResult<()> {
    tokio::fs::rename(temp_path, dest).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to rename '{}' to '{}': {error}",
                temp_path.display(),
                dest.display()
            ),
        )
    })
}

/// Copy one file to another path, creating parent directories as needed.
pub async fn copy(from: &Path, to: &Path) -> AppResult<u64> {
    create_parent_dir(to).await?;
    tokio::fs::copy(from, to).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to copy '{}' to '{}': {error}",
                from.display(),
                to.display()
            ),
        )
    })
}

/// Rename or move a file, creating the destination parent directory as needed.
pub async fn rename(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to).await?;
    tokio::fs::rename(from, to).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to rename '{}' to '{}': {error}",
                from.display(),
                to.display()
            ),
        )
    })
}

/// Move a file, falling back to copy+delete when a platform rename cannot cross filesystems.
///
/// This fallback is not atomic across filesystems. Use [`rename`] when atomic
/// same-filesystem replacement is required.
pub async fn move_file(from: &Path, to: &Path) -> AppResult<()> {
    create_parent_dir(to).await?;
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(error) if is_cross_device_error(&error) => {
            copy(from, to).await?;
            remove(from).await
        }
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to move '{}' to '{}': {error}",
                from.display(),
                to.display()
            ),
        )),
    }
}

/// Remove a file.
pub async fn remove(path: &Path) -> AppResult<()> {
    tokio::fs::remove_file(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to remove '{}': {error}", path.display()),
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

/// Remove a file and ignore `NotFound`.
pub async fn remove_file_if_exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to remove '{}': {error}", path.display()),
        )),
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
    create_parent_dir(dest).await?;
    let temp_path = sibling_temp_path(dest, temp_prefix, ".tmp");
    let result = async {
        tokio::fs::write(&temp_path, bytes).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to write temp file '{}': {error}",
                    temp_path.display()
                ),
            )
        })?;
        persist_temp_file(&temp_path, dest).await
    }
    .await;

    if result.is_err() {
        let _ = remove_file_if_exists(&temp_path).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        copy, exists, metadata, read, read_string, remove_file_if_exists, rename, write,
        write_atomic,
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
        assert!(remove_file_if_exists(&renamed).await.unwrap());
        assert!(!remove_file_if_exists(&renamed).await.unwrap());
    }

    #[tokio::test]
    async fn atomic_write_creates_parent_dirs() {
        let root = TempDir::new().unwrap();
        let path = root.child("nested/file.txt").unwrap();

        write_atomic(&path, b"atomic", "test").await.unwrap();

        assert_eq!(read_string(&path).await.unwrap(), "atomic");
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

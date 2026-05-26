//! Directory helpers.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Metadata for an entry directly inside a directory.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry path.
    pub path: PathBuf,
    /// Entry file name.
    pub file_name: String,
    /// Whether the entry is a regular file.
    pub is_file: bool,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symlink.
    pub is_symlink: bool,
}

/// Create a directory tree if it does not exist.
pub async fn create_dir_all(path: &Path) -> AppResult<()> {
    tokio::fs::create_dir_all(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to create directory '{}': {error}", path.display()),
        )
    })
}

/// Return true when `path` exists as a directory, without following symlinks.
pub async fn exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to inspect directory '{}': {error}", path.display()),
        )),
    }
}

/// List entries directly inside a directory.
pub async fn list(path: &Path) -> AppResult<Vec<DirEntry>> {
    let mut entries = tokio::fs::read_dir(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read directory '{}': {error}", path.display()),
        )
    })?;
    let mut result = Vec::new();

    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read directory entry: {error}"),
        )
    })? {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type().await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to inspect directory entry '{}': {error}",
                    path.display()
                ),
            )
        })?;

        result.push(DirEntry {
            path,
            file_name,
            is_file: file_type.is_file(),
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
        });
    }

    Ok(result)
}

/// Return true when a directory exists and has no entries.
pub async fn is_empty(path: &Path) -> AppResult<bool> {
    Ok(list(path).await?.is_empty())
}

/// Remove an empty directory.
pub async fn remove(path: &Path) -> AppResult<()> {
    tokio::fs::remove_dir(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to remove directory '{}': {error}", path.display()),
        )
    })
}

/// Remove an empty directory and ignore `NotFound`.
pub async fn remove_if_exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::remove_dir(path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to remove directory '{}': {error}", path.display()),
        )),
    }
}

/// Remove a directory tree recursively.
pub async fn remove_all(path: &Path) -> AppResult<()> {
    tokio::fs::remove_dir_all(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to remove directory tree '{}': {error}",
                path.display()
            ),
        )
    })
}

/// Remove a directory tree recursively and ignore `NotFound`.
pub async fn remove_all_if_exists(path: &Path) -> AppResult<bool> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to remove directory tree '{}': {error}",
                path.display()
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{create_dir_all, exists, is_empty, list, remove_all_if_exists, remove_if_exists};
    use crate::TempDir;

    #[tokio::test]
    async fn directory_lifecycle_and_listing() {
        let root = TempDir::new().unwrap();
        let dir = root.child("nested").unwrap();
        create_dir_all(&dir).await.unwrap();
        assert!(exists(&dir).await.unwrap());
        assert!(is_empty(&dir).await.unwrap());

        root.write_file("nested/file.txt", b"hello").unwrap();
        let entries = list(&dir).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name, "file.txt");
        assert!(entries[0].is_file);
        assert!(!is_empty(&dir).await.unwrap());
    }

    #[tokio::test]
    async fn remove_helpers_ignore_missing() {
        let root = TempDir::new().unwrap();
        let missing = root.child("missing").unwrap();
        assert!(!remove_if_exists(&missing).await.unwrap());
        assert!(!remove_all_if_exists(&missing).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exists_rejects_symlinks_to_directories() {
        let root = TempDir::new().unwrap();
        let dir = root.child("dir").unwrap();
        let link = root.child("link").unwrap();
        create_dir_all(&dir).await.unwrap();
        std::os::unix::fs::symlink(&dir, &link).unwrap();

        assert!(!exists(&link).await.unwrap());
    }
}

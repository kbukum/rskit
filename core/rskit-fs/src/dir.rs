//! Directory helpers.
#![allow(clippy::needless_pass_by_value)]

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
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| create_dir_error(path, error))
}

/// Return true when `path` exists as a directory, without following symlinks.
pub async fn exists(path: &Path) -> AppResult<bool> {
    exists_from_metadata(path, tokio::fs::symlink_metadata(path).await)
}

fn exists_from_metadata(
    path: &Path,
    result: std::io::Result<std::fs::Metadata>,
) -> AppResult<bool> {
    match result {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(inspect_dir_error(path, error)),
    }
}

/// List entries directly inside a directory.
pub async fn list(path: &Path) -> AppResult<Vec<DirEntry>> {
    let mut entries = tokio::fs::read_dir(path)
        .await
        .map_err(|error| read_dir_error(path, error))?;
    let mut result = Vec::new();

    while let Some(entry) = entries.next_entry().await.map_err(read_dir_entry_error)? {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry
            .file_type()
            .await
            .map_err(|error| inspect_dir_entry_error(&path, error))?;

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

fn create_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create directory '{}': {error}", path.display()),
    )
}

fn inspect_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to inspect directory '{}': {error}", path.display()),
    )
}

fn read_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory '{}': {error}", path.display()),
    )
}

fn read_dir_entry_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory entry: {error}"),
    )
}

fn inspect_dir_entry_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to inspect directory entry '{}': {error}",
            path.display()
        ),
    )
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
    use super::{
        create_dir_all, create_dir_error, exists, exists_from_metadata, inspect_dir_entry_error,
        inspect_dir_error, is_empty, list, read_dir_entry_error, read_dir_error, remove,
        remove_all, remove_all_if_exists, remove_if_exists,
    };
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
        assert!(!entries[0].is_dir);
        assert!(!entries[0].is_symlink);
        assert!(!is_empty(&dir).await.unwrap());
    }

    #[tokio::test]
    async fn remove_helpers_ignore_missing() {
        let root = TempDir::new().unwrap();
        let missing = root.child("missing").unwrap();
        assert!(!remove_if_exists(&missing).await.unwrap());
        assert!(!remove_all_if_exists(&missing).await.unwrap());
    }

    #[tokio::test]
    async fn remove_helpers_remove_existing_directories() {
        let root = TempDir::new().unwrap();
        let empty = root.child("empty").unwrap();
        let tree = root.child("tree").unwrap();
        create_dir_all(&empty).await.unwrap();
        create_dir_all(&tree.join("nested")).await.unwrap();

        assert!(remove_if_exists(&empty).await.unwrap());
        assert!(remove_all_if_exists(&tree).await.unwrap());
    }

    #[tokio::test]
    async fn remove_and_remove_all_delete_directories() {
        let root = TempDir::new().unwrap();
        let empty = root.child("empty").unwrap();
        let tree = root.child("tree").unwrap();
        create_dir_all(&empty).await.unwrap();
        create_dir_all(&tree.join("nested")).await.unwrap();

        remove(&empty).await.unwrap();
        remove_all(&tree).await.unwrap();

        assert!(!exists(&empty).await.unwrap());
        assert!(!exists(&tree).await.unwrap());
    }

    #[tokio::test]
    async fn directory_errors_are_reported() {
        let root = TempDir::new().unwrap();
        let file = root.write_file("file.txt", b"hello").unwrap();
        let nested_under_file = file.join("child");

        assert!(create_dir_all(&nested_under_file).await.is_err());
        assert!(!exists(&root.child("missing").unwrap()).await.unwrap());
        assert!(list(&file).await.is_err());
        assert!(remove(&file).await.is_err());
        assert!(remove_if_exists(&file).await.is_err());
        assert!(remove_all(&file).await.is_err());
        assert!(remove_all_if_exists(&file).await.is_err());
    }

    #[test]
    fn directory_error_builders_include_context() {
        let path = std::path::Path::new("dir");
        let err = || std::io::Error::other("boom");

        assert!(
            create_dir_error(path, err())
                .to_string()
                .contains("create directory")
        );
        assert!(
            inspect_dir_error(path, err())
                .to_string()
                .contains("inspect directory")
        );
        assert!(
            read_dir_error(path, err())
                .to_string()
                .contains("read directory")
        );
        assert!(
            read_dir_entry_error(err())
                .to_string()
                .contains("directory entry")
        );
        assert!(
            inspect_dir_entry_error(path, err())
                .to_string()
                .contains("inspect directory entry")
        );
        assert!(exists_from_metadata(path, Err(err())).is_err());
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

    #[cfg(unix)]
    #[tokio::test]
    async fn list_marks_symlink_entries() {
        let root = TempDir::new().unwrap();
        let dir = root.child("dir").unwrap();
        let link = root.child("link").unwrap();
        create_dir_all(&dir).await.unwrap();
        std::os::unix::fs::symlink(&dir, &link).unwrap();

        let entries = list(root.path()).await.unwrap();
        let link_entry = entries
            .iter()
            .find(|entry| entry.file_name == "link")
            .unwrap();
        assert!(link_entry.is_symlink);
        assert!(!link_entry.is_file);
        assert!(!link_entry.is_dir);
    }
}

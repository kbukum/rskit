//! Hard-link and symbolic-link helpers.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Create a hard link.
pub async fn hard_link(original: &Path, link: &Path) -> AppResult<()> {
    tokio::fs::hard_link(original, link).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to create hard link '{}' -> '{}': {error}",
                link.display(),
                original.display()
            ),
        )
    })
}

/// Read a symbolic link target.
pub async fn read_link(path: &Path) -> AppResult<PathBuf> {
    tokio::fs::read_link(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read link '{}': {error}", path.display()),
        )
    })
}

/// Create a symbolic link to a file.
#[cfg(unix)]
pub async fn symlink_file(original: &Path, link: &Path) -> AppResult<()> {
    let original = original.to_path_buf();
    let link = link.to_path_buf();
    let symlink_original = original.clone();
    let symlink_link = link.clone();
    tokio::task::spawn_blocking(move || {
        std::os::unix::fs::symlink(&symlink_original, &symlink_link)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to create symlink '{}' -> '{}': {error}",
                link.display(),
                original.display()
            ),
        )
    })
}

/// Create a symbolic link to a directory.
#[cfg(unix)]
pub async fn symlink_dir(original: &Path, link: &Path) -> AppResult<()> {
    symlink_file(original, link).await
}

#[cfg(test)]
mod tests {
    use super::{hard_link, read_link};
    use crate::{TempDir, file};

    #[tokio::test]
    async fn creates_hard_link() {
        let dir = TempDir::new().unwrap();
        let original = dir.child("original.txt").unwrap();
        let link = dir.child("link.txt").unwrap();
        file::write(&original, b"hello").await.unwrap();

        hard_link(&original, &link).await.unwrap();

        assert_eq!(file::read_string(&link).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn link_errors_are_reported() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();
        let link = dir.child("link.txt").unwrap();

        assert!(hard_link(&missing, &link).await.is_err());
        assert!(read_link(&missing).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn creates_and_reads_symlink() {
        let dir = TempDir::new().unwrap();
        let original = dir.child("original.txt").unwrap();
        let link = dir.child("link.txt").unwrap();
        file::write(&original, b"hello").await.unwrap();

        super::symlink_file(&original, &link).await.unwrap();

        assert_eq!(read_link(&link).await.unwrap(), original);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn creates_directory_symlink() {
        let dir = TempDir::new().unwrap();
        let original = dir.child("original").unwrap();
        let link = dir.child("link").unwrap();
        crate::dir::create_dir_all(&original).await.unwrap();

        super::symlink_dir(&original, &link).await.unwrap();

        assert_eq!(read_link(&link).await.unwrap(), original);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_errors_are_reported() {
        let dir = TempDir::new().unwrap();
        let original = dir.child("original.txt").unwrap();
        let link = dir.child("link.txt").unwrap();
        file::write(&original, b"hello").await.unwrap();
        file::write(&link, b"exists").await.unwrap();

        assert!(super::symlink_file(&original, &link).await.is_err());
    }
}

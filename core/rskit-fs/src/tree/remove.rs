//! Tree removal.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Remove a directory tree recursively.
pub fn remove_tree(path: &Path) -> AppResult<()> {
    std::fs::remove_dir_all(path).map_err(|error| {
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
pub fn remove_tree_if_exists(path: &Path) -> AppResult<bool> {
    match std::fs::remove_dir_all(path) {
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
    use super::{remove_tree, remove_tree_if_exists};
    use crate::TempDir;

    #[test]
    fn remove_tree_if_exists_ignores_missing() {
        let dir = TempDir::new().unwrap();
        let target = dir.child("missing").unwrap();

        assert!(!remove_tree_if_exists(&target).unwrap());
    }

    #[test]
    fn remove_tree_removes_existing_tree() {
        let dir = TempDir::new().unwrap();
        let target = dir.child("tree").unwrap();
        std::fs::create_dir_all(target.join("nested")).unwrap();

        remove_tree(&target).unwrap();

        assert!(!target.exists());
    }

    #[test]
    fn remove_tree_if_exists_removes_existing_tree() {
        let dir = TempDir::new().unwrap();
        let target = dir.child("tree").unwrap();
        std::fs::create_dir_all(target.join("nested")).unwrap();

        assert!(remove_tree_if_exists(&target).unwrap());
        assert!(!target.exists());
    }

    #[test]
    fn remove_tree_errors_on_files() {
        let dir = TempDir::new().unwrap();
        let target = dir.write_file("file.txt", b"hello").unwrap();

        assert!(remove_tree(&target).is_err());
        assert!(remove_tree_if_exists(&target).is_err());
    }
}

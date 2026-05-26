//! Tree listing.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{
    TreeEntry, VisitedDirs, ensure_directory, enter_directory, init_visited_dirs, metadata_for,
};

/// List every entry in a directory tree.
///
/// Set `follow_symlinks` only when the caller intentionally trusts symlink
/// targets. Leaving it `false` prevents traversal outside the requested tree.
pub fn list_tree(root: &Path, follow_symlinks: bool) -> AppResult<Vec<TreeEntry>> {
    ensure_directory(root)?;

    let mut entries = Vec::new();
    let mut visited = init_visited_dirs(root)?;
    list_tree_recursive(root, root, follow_symlinks, &mut visited, &mut entries)?;
    Ok(entries)
}

fn list_tree_recursive(
    root: &Path,
    current: &Path,
    follow_symlinks: bool,
    visited: &mut VisitedDirs,
    entries: &mut Vec<TreeEntry>,
) -> AppResult<()> {
    for entry in std::fs::read_dir(current).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read directory '{}': {error}", current.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read directory entry: {error}"),
            )
        })?;
        let path = entry.path();
        let metadata = metadata_for(&path, follow_symlinks)?;
        let file_type = metadata.file_type();
        let relative_path = path.strip_prefix(root).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to strip prefix: {error}"),
            )
        })?;

        entries.push(TreeEntry {
            path: path.clone(),
            relative_path: relative_path.to_path_buf(),
            is_file: file_type.is_file(),
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
        });

        if file_type.is_dir() {
            enter_directory(&path, visited)?;
            list_tree_recursive(root, &path, follow_symlinks, visited, entries)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::list_tree;
    use crate::TempDir;

    #[test]
    fn list_tree_returns_relative_paths() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        source.write_file("nested/b.txt", b"beta").unwrap();

        let mut entries = list_tree(source.path(), false)
            .unwrap()
            .into_iter()
            .map(|entry| entry.relative_path)
            .collect::<Vec<_>>();
        entries.sort();

        assert_eq!(
            entries,
            vec![
                std::path::PathBuf::from("a.txt"),
                std::path::PathBuf::from("nested"),
                std::path::PathBuf::from("nested/b.txt"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn list_tree_rejects_symlink_cycles_when_following() {
        let source = TempDir::new().unwrap();
        std::fs::create_dir_all(source.child("nested").unwrap()).unwrap();
        std::os::unix::fs::symlink(source.path(), source.child("nested/back").unwrap()).unwrap();

        assert!(list_tree(source.path(), true).is_err());
    }
}

//! Tree listing.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{
    TreeEntry, VisitedDirs, ensure_directory, enter_directory, init_visited_dirs, metadata_for,
};

/// List every entry in a directory tree.
///
/// This helper uses blocking `std::fs` I/O. Use `tokio::task::spawn_blocking`
/// or an equivalent blocking executor boundary when calling it from async code.
///
/// Set `follow_symlinks` only when the caller intentionally trusts symlink targets.
/// Leaving it `false` prevents traversal outside the requested tree.
pub fn list_tree(root: &Path, follow_symlinks: bool) -> AppResult<Vec<TreeEntry>> {
    ensure_directory(root, follow_symlinks)?;

    let mut entries = Vec::new();
    let mut visited = init_visited_dirs(root, follow_symlinks)?;
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
    for entry in std::fs::read_dir(current).map_err(|error| read_list_dir_error(current, error))? {
        let entry = entry.map_err(read_list_dir_entry_error)?;
        let path = entry.path();
        let metadata = metadata_for(&path, follow_symlinks)?;
        let file_type = metadata.file_type();
        let relative_path = path.strip_prefix(root).map_err(strip_list_prefix_error)?;

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

fn read_list_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory '{}': {error}", path.display()),
    )
}

fn read_list_dir_entry_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory entry: {error}"),
    )
}

fn strip_list_prefix_error(error: std::path::StripPrefixError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to strip prefix: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        list_tree, list_tree_recursive, read_list_dir_entry_error, read_list_dir_error,
        strip_list_prefix_error,
    };
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

    #[test]
    fn list_tree_reports_read_dir_errors() {
        let source = TempDir::new().unwrap();
        let file = source.write_file("file.txt", b"hello").unwrap();
        let mut visited = super::super::init_visited_dirs(source.path(), false).unwrap();
        let mut entries = Vec::new();

        assert!(
            list_tree_recursive(source.path(), &file, false, &mut visited, &mut entries).is_err()
        );
    }

    #[test]
    fn list_tree_reports_strip_prefix_errors() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        outside.write_file("file.txt", b"hello").unwrap();
        let mut visited = super::super::init_visited_dirs(root.path(), false).unwrap();
        let mut entries = Vec::new();

        assert!(
            list_tree_recursive(
                root.path(),
                outside.path(),
                false,
                &mut visited,
                &mut entries
            )
            .is_err()
        );
    }

    #[test]
    fn list_tree_error_builders_include_context() {
        let path = std::path::Path::new("dir");
        let err = || std::io::Error::other("boom");

        assert!(
            read_list_dir_error(path, err())
                .to_string()
                .contains("read directory")
        );
        assert!(
            read_list_dir_entry_error(err())
                .to_string()
                .contains("directory entry")
        );

        let strip_error = std::path::Path::new("a").strip_prefix("b").unwrap_err();
        assert!(
            strip_list_prefix_error(strip_error)
                .to_string()
                .contains("strip prefix")
        );
    }
}

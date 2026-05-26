//! Tree walking.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{
    TreeEntry, VisitedDirs, WalkControl, WalkOptions, ensure_directory, enter_directory,
    init_visited_dirs, metadata_for,
};

/// Walk a directory tree without allocating a full tree listing.
///
/// The callback receives entries in pre-order. Symlinks are not followed unless
/// [`WalkOptions::follow_symlinks`] is enabled.
pub fn walk_tree(
    root: &Path,
    options: WalkOptions,
    mut visitor: impl FnMut(&TreeEntry) -> AppResult<WalkControl>,
) -> AppResult<()> {
    ensure_directory(root)?;
    let mut visited = init_visited_dirs(root)?;
    walk_tree_recursive(root, root, options, &mut visited, &mut visitor).map(drop)
}

fn walk_tree_recursive(
    root: &Path,
    current: &Path,
    options: WalkOptions,
    visited: &mut VisitedDirs,
    visitor: &mut impl FnMut(&TreeEntry) -> AppResult<WalkControl>,
) -> AppResult<bool> {
    for entry in std::fs::read_dir(current).map_err(|error| read_walk_dir_error(current, error))? {
        let entry = entry.map_err(read_walk_dir_entry_error)?;
        let path = entry.path();
        let metadata = metadata_for(&path, options.follow_symlinks)?;
        let file_type = metadata.file_type();
        let relative_path = path.strip_prefix(root).map_err(strip_walk_prefix_error)?;

        let tree_entry = TreeEntry {
            path: path.clone(),
            relative_path: relative_path.to_path_buf(),
            is_file: file_type.is_file(),
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
        };

        let mut should_descend = file_type.is_dir();
        if should_visit(&tree_entry, options) {
            match visitor(&tree_entry)? {
                WalkControl::Continue => {}
                WalkControl::SkipSubtree => should_descend = false,
                WalkControl::Stop => return Ok(true),
            }
        }

        if should_descend {
            enter_directory(&path, visited)?;
            if walk_tree_recursive(root, &path, options, visited, visitor)? {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

const fn should_visit(entry: &TreeEntry, options: WalkOptions) -> bool {
    (entry.is_dir && options.entry_filter.includes_dirs())
        || (entry.is_file && options.entry_filter.includes_files())
        || (entry.is_symlink && options.entry_filter.includes_symlinks())
}

fn read_walk_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory '{}': {error}", path.display()),
    )
}

fn read_walk_dir_entry_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory entry: {error}"),
    )
}

fn strip_walk_prefix_error(error: std::path::StripPrefixError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to strip prefix: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use rskit_errors::AppResult;

    use super::{
        read_walk_dir_entry_error, read_walk_dir_error, strip_walk_prefix_error, walk_tree,
        walk_tree_recursive,
    };
    use crate::TempDir;
    use crate::tree::{TreeEntry, WalkControl, WalkOptions};

    #[allow(clippy::unnecessary_wraps)]
    fn continue_walk(_: &TreeEntry) -> AppResult<WalkControl> {
        Ok(WalkControl::Continue)
    }

    #[test]
    fn walk_tree_visits_entries_without_allocating_result() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        source.write_file("nested/b.txt", b"beta").unwrap();
        let mut visited = Vec::new();

        walk_tree(source.path(), WalkOptions::default(), |entry| {
            visited.push(entry.relative_path.clone());
            Ok(WalkControl::Continue)
        })
        .unwrap();
        visited.sort();

        assert_eq!(
            visited,
            vec![
                std::path::PathBuf::from("a.txt"),
                std::path::PathBuf::from("nested"),
                std::path::PathBuf::from("nested/b.txt"),
            ]
        );
    }

    #[test]
    fn walk_tree_can_skip_subtrees() {
        let source = TempDir::new().unwrap();
        source.write_file("nested/b.txt", b"beta").unwrap();
        let mut visited = Vec::new();
        source.write_file("top.txt", b"alpha").unwrap();

        walk_tree(
            source.path(),
            WalkOptions::default(),
            |entry| -> AppResult<_> {
                visited.push(entry.relative_path.clone());
                if entry.relative_path == std::path::Path::new("nested") {
                    return Ok(WalkControl::SkipSubtree);
                }

                Ok(WalkControl::Continue)
            },
        )
        .unwrap();

        visited.sort();
        assert_eq!(
            visited,
            vec![
                std::path::PathBuf::from("nested"),
                std::path::PathBuf::from("top.txt"),
            ]
        );
    }

    #[test]
    fn walk_tree_error_builders_include_context() {
        let path = std::path::Path::new("dir");
        let err = || std::io::Error::other("boom");

        assert!(
            continue_walk(&TreeEntry {
                path: path.to_path_buf(),
                relative_path: path.to_path_buf(),
                is_file: true,
                is_dir: false,
                is_symlink: false,
            })
            .is_ok()
        );

        assert!(
            read_walk_dir_error(path, err())
                .to_string()
                .contains("read directory")
        );
        assert!(
            read_walk_dir_entry_error(err())
                .to_string()
                .contains("directory entry")
        );

        let strip_error = std::path::Path::new("a").strip_prefix("b").unwrap_err();
        assert!(
            strip_walk_prefix_error(strip_error)
                .to_string()
                .contains("strip prefix")
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_tree_rejects_symlink_cycles_when_following() {
        let source = TempDir::new().unwrap();
        std::fs::create_dir_all(source.child("nested").unwrap()).unwrap();
        std::os::unix::fs::symlink(source.path(), source.child("nested/back").unwrap()).unwrap();

        assert!(
            walk_tree(
                source.path(),
                WalkOptions {
                    follow_symlinks: true,
                    ..WalkOptions::default()
                },
                |_| Ok(WalkControl::Continue),
            )
            .is_err()
        );
    }

    #[test]
    fn walk_tree_can_stop_early() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        source.write_file("b.txt", b"beta").unwrap();
        let mut visits = 0;

        walk_tree(source.path(), WalkOptions::default(), |_| {
            visits += 1;
            Ok(WalkControl::Stop)
        })
        .unwrap();

        assert_eq!(visits, 1);
    }

    #[test]
    fn walk_tree_stop_propagates_from_nested_directory() {
        let source = TempDir::new().unwrap();
        source.write_file("nested/stop.txt", b"stop").unwrap();
        let mut visited = super::super::init_visited_dirs(source.path()).unwrap();
        let mut visitor = |entry: &TreeEntry| {
            if entry.relative_path == std::path::Path::new("nested/stop.txt") {
                return Ok(WalkControl::Stop);
            }

            Ok(WalkControl::Continue)
        };

        assert!(
            walk_tree_recursive(
                source.path(),
                source.path(),
                WalkOptions::default(),
                &mut visited,
                &mut visitor,
            )
            .unwrap()
        );
    }

    #[test]
    fn walk_tree_filters_entry_kinds() {
        let source = TempDir::new().unwrap();
        source.write_file("nested/file.txt", b"hello").unwrap();
        let mut visited = Vec::new();

        walk_tree(
            source.path(),
            WalkOptions {
                entry_filter: crate::tree::WalkEntryFilter::FILES,
                ..WalkOptions::default()
            },
            |entry| {
                visited.push(entry.relative_path.clone());
                Ok(WalkControl::Continue)
            },
        )
        .unwrap();

        assert_eq!(visited, vec![std::path::PathBuf::from("nested/file.txt")]);
    }

    #[cfg(unix)]
    #[test]
    fn walk_tree_visits_symlinks_when_filtered() {
        let source = TempDir::new().unwrap();
        let target = source.write_file("target.txt", b"hello").unwrap();
        let link = source.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut visited = Vec::new();

        walk_tree(
            source.path(),
            WalkOptions {
                entry_filter: crate::tree::WalkEntryFilter::SYMLINKS,
                ..WalkOptions::default()
            },
            |entry| {
                visited.push(entry.relative_path.clone());
                Ok(WalkControl::Continue)
            },
        )
        .unwrap();

        assert_eq!(visited, vec![std::path::PathBuf::from("link.txt")]);
    }

    #[test]
    fn walk_tree_reports_read_dir_errors() {
        let source = TempDir::new().unwrap();
        let file = source.write_file("file.txt", b"hello").unwrap();
        let mut visited = super::super::init_visited_dirs(source.path()).unwrap();
        let mut visitor = continue_walk;

        assert!(
            walk_tree_recursive(
                source.path(),
                &file,
                WalkOptions::default(),
                &mut visited,
                &mut visitor,
            )
            .is_err()
        );
    }

    #[test]
    fn walk_tree_reports_strip_prefix_errors() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        outside.write_file("file.txt", b"hello").unwrap();
        let mut visited = super::super::init_visited_dirs(root.path()).unwrap();
        let mut visitor = continue_walk;

        assert!(
            walk_tree_recursive(
                root.path(),
                outside.path(),
                WalkOptions::default(),
                &mut visited,
                &mut visitor,
            )
            .is_err()
        );
    }
}

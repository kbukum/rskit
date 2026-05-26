//! Tree walking.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{TreeEntry, WalkControl, WalkOptions, ensure_directory, metadata_for};

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
    walk_tree_recursive(root, root, options, &mut visitor)
}

fn walk_tree_recursive(
    root: &Path,
    current: &Path,
    options: WalkOptions,
    visitor: &mut impl FnMut(&TreeEntry) -> AppResult<WalkControl>,
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
        let metadata = metadata_for(&path, options.follow_symlinks)?;
        let file_type = metadata.file_type();
        let relative_path = path.strip_prefix(root).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to strip prefix: {error}"),
            )
        })?;

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
                WalkControl::Stop => return Ok(()),
            }
        }

        if should_descend {
            walk_tree_recursive(root, &path, options, visitor)?;
        }
    }

    Ok(())
}

fn should_visit(entry: &TreeEntry, options: WalkOptions) -> bool {
    (entry.is_dir && options.include_dirs)
        || (entry.is_file && options.include_files)
        || (entry.is_symlink && options.include_symlinks)
}

#[cfg(test)]
mod tests {
    use rskit_errors::AppResult;

    use super::walk_tree;
    use crate::TempDir;
    use crate::tree::{WalkControl, WalkOptions};

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

        assert_eq!(visited, vec![std::path::PathBuf::from("nested")]);
    }
}

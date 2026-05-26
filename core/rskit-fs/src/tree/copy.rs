//! Tree copying.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{
    CopyTreeOptions, VisitedDirs, canonical_dir, ensure_directory, enter_directory,
    init_visited_dirs, metadata_for,
};
use crate::path::safe_join;

/// Copy a directory tree from `source` into `dest`.
///
/// Directory structure is preserved. Symlinks are skipped by default to avoid
/// accidentally copying content outside the requested source tree.
pub fn copy_tree(source: &Path, dest: &Path, options: CopyTreeOptions) -> AppResult<()> {
    ensure_directory(source)?;

    std::fs::create_dir_all(dest).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to create destination '{}': {error}", dest.display()),
        )
    })?;
    ensure_destination_outside_source(source, dest)?;

    let mut visited = init_visited_dirs(source)?;
    copy_tree_recursive(source, source, dest, options, &mut visited)
}

fn ensure_destination_outside_source(source: &Path, dest: &Path) -> AppResult<()> {
    let source = canonical_dir(source)?;
    let dest = canonical_dir(dest)?;
    if dest.starts_with(&source) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "destination '{}' must not be inside source '{}'",
                dest.display(),
                source.display()
            ),
        ));
    }
    Ok(())
}

fn copy_tree_recursive(
    root: &Path,
    current: &Path,
    dest: &Path,
    options: CopyTreeOptions,
    visited: &mut VisitedDirs,
) -> AppResult<()> {
    let entries = std::fs::read_dir(current).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read directory '{}': {error}", current.display()),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read directory entry: {error}"),
            )
        })?;
        let path = entry.path();
        let metadata = metadata_for(&path, options.follow_symlinks)?;
        let file_type = metadata.file_type();
        let rel = path.strip_prefix(root).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to strip prefix: {error}"),
            )
        })?;
        let target = safe_join(dest, rel).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("destination path escaped root: {error}"),
            )
        })?;

        if file_type.is_symlink() && !options.follow_symlinks {
            continue;
        }
        if file_type.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create directory '{}': {error}", target.display()),
                )
            })?;
            enter_directory(&path, visited)?;
            copy_tree_recursive(root, &path, dest, options, visited)?;
        } else if file_type.is_file() {
            if target.exists() && !options.overwrite {
                return Err(AppError::new(
                    ErrorCode::AlreadyExists,
                    format!("destination file already exists: {}", target.display()),
                ));
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "failed to create parent directory '{}': {error}",
                            parent.display()
                        ),
                    )
                })?;
            }
            std::fs::copy(&path, &target).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to copy '{}' to '{}': {error}",
                        path.display(),
                        target.display()
                    ),
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::copy_tree;
    use crate::TempDir;
    use crate::tree::CopyTreeOptions;

    #[test]
    fn copy_tree_copies_nested_files() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        source.write_file("nested/b.txt", b"beta").unwrap();
        let dest = TempDir::new().unwrap();

        copy_tree(source.path(), dest.path(), CopyTreeOptions::default()).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.child("a.txt").unwrap()).unwrap(),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(dest.child("nested/b.txt").unwrap()).unwrap(),
            "beta"
        );
    }

    #[test]
    fn copy_tree_rejects_missing_source() {
        let dest = TempDir::new().unwrap();
        let err = copy_tree(
            std::path::Path::new("/missing-rskit-fs-copy-tree-source"),
            dest.path(),
            CopyTreeOptions::default(),
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn copy_tree_respects_no_overwrite() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"new").unwrap();
        let dest = TempDir::new().unwrap();
        dest.write_file("a.txt", b"old").unwrap();

        let err = copy_tree(
            source.path(),
            dest.path(),
            CopyTreeOptions {
                overwrite: false,
                ..CopyTreeOptions::default()
            },
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(dest.child("a.txt").unwrap()).unwrap(),
            "old"
        );
    }

    #[test]
    fn copy_tree_rejects_destination_inside_source() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        let dest = source.child("nested/dest").unwrap();

        let err = copy_tree(source.path(), &dest, CopyTreeOptions::default()).unwrap_err();

        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_rejects_symlink_cycles_when_following() {
        let source = TempDir::new().unwrap();
        std::fs::create_dir_all(source.child("nested").unwrap()).unwrap();
        std::os::unix::fs::symlink(source.path(), source.child("nested/back").unwrap()).unwrap();
        let dest = TempDir::new().unwrap();

        assert!(
            copy_tree(
                source.path(),
                dest.path(),
                CopyTreeOptions {
                    follow_symlinks: true,
                    ..CopyTreeOptions::default()
                },
            )
            .is_err()
        );
    }
}

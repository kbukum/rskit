//! Tree copying.
#![allow(clippy::needless_pass_by_value)]

use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{
    CopyTreeOptions, VisitedDirs, canonical_dir, ensure_directory, enter_directory,
    init_visited_dirs, metadata_for,
};
use crate::path::{absolute, safe_join};

/// Copy a directory tree from `source` into `dest`.
///
/// This helper uses blocking `std::fs` I/O. Use `tokio::task::spawn_blocking`
/// or an equivalent blocking executor boundary when calling it from async code.
///
/// Directory structure is preserved. Symlinks are skipped by default to avoid
/// accidentally copying content outside the requested source tree.
pub fn copy_tree(source: &Path, dest: &Path, options: CopyTreeOptions) -> AppResult<()> {
    ensure_directory(source, options.follow_symlinks)?;
    ensure_destination_outside_source(source, dest)?;

    std::fs::create_dir_all(dest).map_err(|error| create_destination_error(dest, error))?;

    let mut visited = init_visited_dirs(source, options.follow_symlinks)?;
    copy_tree_recursive(source, source, dest, options, &mut visited)
}

fn ensure_destination_outside_source(source: &Path, dest: &Path) -> AppResult<()> {
    let source = canonical_dir(source)?;
    let dest = canonical_target_path(dest)?;
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

fn canonical_target_path(path: &Path) -> AppResult<PathBuf> {
    if path.exists() {
        return canonicalize_destination(path);
    }

    let absolute_path = absolute(path)?;
    let current = absolute_path
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or(absolute_path.as_path());
    let mut canonical = canonicalize_destination_parent(current)?;
    for component in absolute_path
        .components()
        .skip(current.components().count())
    {
        match component {
            Component::Normal(component) => canonical.push(component),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid_destination_component_error(path));
            }
        }
    }
    Ok(canonical)
}

fn canonicalize_destination(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize destination '{}': {error}",
                path.display()
            ),
        )
    })
}

fn canonicalize_destination_parent(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize destination parent '{}': {error}",
                path.display()
            ),
        )
    })
}

fn invalid_destination_component_error(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "destination '{}' must not contain unresolved parent, root, or prefix components",
            path.display()
        ),
    )
}

fn copy_tree_recursive(
    root: &Path,
    current: &Path,
    dest: &Path,
    options: CopyTreeOptions,
    visited: &mut VisitedDirs,
) -> AppResult<()> {
    let entries =
        std::fs::read_dir(current).map_err(|error| read_copy_dir_error(current, error))?;

    for entry in entries {
        let entry = entry.map_err(read_copy_dir_entry_error)?;
        let path = entry.path();
        let metadata = metadata_for(&path, options.follow_symlinks)?;
        let file_type = metadata.file_type();
        let rel = path.strip_prefix(root).map_err(strip_copy_prefix_error)?;
        let target = safe_join(dest, rel).map_err(copy_destination_escaped_error)?;

        if file_type.is_symlink() && !options.follow_symlinks {
            continue;
        }
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|error| create_copied_directory_error(&target, error))?;
            enter_directory(&path, visited)?;
            copy_tree_recursive(root, &path, dest, options, visited)?;
        } else if file_type.is_file() {
            if target.exists() && !options.overwrite {
                return Err(AppError::new(
                    ErrorCode::AlreadyExists,
                    format!("destination file already exists: {}", target.display()),
                ));
            }
            let parent = target.parent().unwrap_or(dest);
            std::fs::create_dir_all(parent)
                .map_err(|error| create_copied_parent_error(parent, error))?;
            std::fs::copy(&path, &target)
                .map_err(|error| copy_tree_file_error(&path, &target, error))?;
        }
    }

    Ok(())
}

fn create_destination_error(dest: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create destination '{}': {error}", dest.display()),
    )
}

fn read_copy_dir_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory '{}': {error}", path.display()),
    )
}

fn read_copy_dir_entry_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to read directory entry: {error}"),
    )
}

fn strip_copy_prefix_error(error: std::path::StripPrefixError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to strip prefix: {error}"),
    )
}

fn copy_destination_escaped_error(error: crate::SafePathError) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("destination path escaped root: {error}"),
    )
}

fn create_copied_directory_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create directory '{}': {error}", path.display()),
    )
}

fn create_copied_parent_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to create parent directory '{}': {error}",
            path.display()
        ),
    )
}

fn copy_tree_file_error(from: &Path, to: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to copy '{}' to '{}': {error}",
            from.display(),
            to.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::{
        canonical_target_path, canonicalize_destination, canonicalize_destination_parent,
        copy_destination_escaped_error, copy_tree, copy_tree_file_error, copy_tree_recursive,
        create_copied_directory_error, create_copied_parent_error, create_destination_error,
        read_copy_dir_entry_error, read_copy_dir_error, strip_copy_prefix_error,
    };
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
    fn copy_tree_error_builders_include_context() {
        let from = std::path::Path::new("from");
        let to = std::path::Path::new("to");
        let err = || std::io::Error::other("boom");

        assert!(
            create_destination_error(to, err())
                .to_string()
                .contains("destination")
        );
        assert!(
            read_copy_dir_error(from, err())
                .to_string()
                .contains("read directory")
        );
        assert!(
            read_copy_dir_entry_error(err())
                .to_string()
                .contains("directory entry")
        );
        assert!(
            create_copied_directory_error(to, err())
                .to_string()
                .contains("create directory")
        );
        assert!(
            create_copied_parent_error(to, err())
                .to_string()
                .contains("parent directory")
        );
        assert!(
            copy_tree_file_error(from, to, err())
                .to_string()
                .contains("copy")
        );
        assert!(
            copy_destination_escaped_error(crate::SafePathError::ParentDir)
                .to_string()
                .contains("escaped root")
        );

        let strip_error = std::path::Path::new("a").strip_prefix("b").unwrap_err();
        assert!(
            strip_copy_prefix_error(strip_error)
                .to_string()
                .contains("strip prefix")
        );
        assert!(canonicalize_destination(std::path::Path::new("missing")).is_err());
        assert!(canonicalize_destination_parent(std::path::Path::new("missing")).is_err());
        assert!(canonical_target_path(std::path::Path::new("missing/child")).is_ok());
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
        assert!(!dest.exists());
    }

    #[test]
    fn copy_tree_rejects_destination_with_unresolved_parent_components() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        let dest = source.path().join("missing/../dest");

        let err = copy_tree(source.path(), &dest, CopyTreeOptions::default()).unwrap_err();

        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(!source.child("dest").unwrap().exists());
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

    #[test]
    fn copy_tree_reports_destination_create_errors() {
        let source = TempDir::new().unwrap();
        source.write_file("a.txt", b"alpha").unwrap();
        let dest_root = TempDir::new().unwrap();
        let dest = dest_root.write_file("dest.txt", b"exists").unwrap();

        assert!(copy_tree(source.path(), &dest, CopyTreeOptions::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_skips_symlinks_by_default() {
        let source = TempDir::new().unwrap();
        let target = source.write_file("target.txt", b"hello").unwrap();
        let link = source.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let dest = TempDir::new().unwrap();

        copy_tree(source.path(), dest.path(), CopyTreeOptions::default()).unwrap();

        assert!(dest.child("target.txt").unwrap().exists());
        assert!(!dest.child("link.txt").unwrap().exists());
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_ignores_special_files() {
        let source = TempDir::new().unwrap();
        let fifo = source.child("pipe").unwrap();
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());
        let dest = TempDir::new().unwrap();

        copy_tree(source.path(), dest.path(), CopyTreeOptions::default()).unwrap();

        assert!(!dest.child("pipe").unwrap().exists());
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_follows_file_symlinks_when_requested() {
        let source = TempDir::new().unwrap();
        let target = source.write_file("target.txt", b"hello").unwrap();
        let link = source.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let dest = TempDir::new().unwrap();

        copy_tree(
            source.path(),
            dest.path(),
            CopyTreeOptions {
                follow_symlinks: true,
                ..CopyTreeOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.child("link.txt").unwrap()).unwrap(),
            "hello"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_reports_broken_symlink_when_following() {
        let source = TempDir::new().unwrap();
        let missing = source.child("missing.txt").unwrap();
        let link = source.child("link.txt").unwrap();
        std::os::unix::fs::symlink(&missing, &link).unwrap();
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

    #[test]
    fn copy_tree_recursive_reports_read_dir_errors() {
        let source = TempDir::new().unwrap();
        let file = source.write_file("file.txt", b"hello").unwrap();
        let dest = TempDir::new().unwrap();
        let mut visited = super::init_visited_dirs(source.path(), false).unwrap();

        assert!(
            copy_tree_recursive(
                source.path(),
                &file,
                dest.path(),
                CopyTreeOptions::default(),
                &mut visited,
            )
            .is_err()
        );
    }

    #[test]
    fn copy_tree_recursive_reports_strip_prefix_errors() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        outside.write_file("file.txt", b"hello").unwrap();
        let dest = TempDir::new().unwrap();
        let mut visited = super::init_visited_dirs(root.path(), false).unwrap();

        assert!(
            copy_tree_recursive(
                root.path(),
                outside.path(),
                dest.path(),
                CopyTreeOptions::default(),
                &mut visited,
            )
            .is_err()
        );
    }
}

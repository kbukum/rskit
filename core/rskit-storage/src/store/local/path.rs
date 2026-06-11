//! Local store path normalization, confinement, and write helpers.

use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::async_io;

use super::super::prefixed_key;

/// Normalize and validate a caller-provided storage key as root-relative.
pub(super) fn normalize_local_key(key: &str) -> AppResult<String> {
    let key = prefixed_key(None, key);
    rskit_fs::validate_relative_path(Path::new(&key)).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("storage key must stay within the configured root ({key}): {error}"),
        )
    })?;
    Ok(key)
}

/// Canonicalize an existing path and reject it if it escapes the configured root.
pub(super) async fn canonicalize_confined(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = canonicalize_root(root).await?;
    let canonical = tokio::fs::canonicalize(path).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize storage path '{}': {error}",
                path.display()
            ),
        )
        .with_cause(error)
    })?;
    ensure_canonical_stays_within_root(&root, &canonical)
}

/// Ensure a write target's parent exists without following symlink escapes.
pub(super) async fn ensure_target_parent_confined(root: &Path, target: &Path) -> AppResult<()> {
    let parent = target.parent().ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "storage target '{}' has no parent directory",
                target.display()
            ),
        )
    })?;
    ensure_existing_parent_prefix_confined(root, parent).await?;
    async_io::dir::create_all(parent).await?;
    ensure_existing_parent_prefix_confined(root, parent).await?;
    canonicalize_confined(root, parent).await?;
    Ok(())
}

async fn ensure_existing_parent_prefix_confined(root: &Path, parent: &Path) -> AppResult<()> {
    let root_canonical = canonicalize_root(root).await?;
    let relative_parent = parent.strip_prefix(root).map_err(|_| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "storage target parent '{}' is not under configured root '{}'",
                parent.display(),
                root.display()
            ),
        )
    })?;

    let mut existing = root.to_path_buf();
    ensure_canonical_stays_within_root(&root_canonical, &root_canonical)?;
    for component in relative_parent.components() {
        match component {
            Component::Normal(name) => existing.push(name),
            Component::CurDir => continue,
            _ => {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "storage target parent '{}' contains an invalid path component",
                        parent.display()
                    ),
                ));
            }
        }

        match tokio::fs::symlink_metadata(&existing).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "storage target parent '{}' must not traverse symlink '{}'",
                            parent.display(),
                            existing.display()
                        ),
                    ));
                }
                if !metadata.is_dir() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "storage target parent component '{}' is not a directory",
                            existing.display()
                        ),
                    ));
                }
                let canonical = tokio::fs::canonicalize(&existing).await.map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "failed to canonicalize storage parent component '{}': {error}",
                            existing.display()
                        ),
                    )
                    .with_cause(error)
                })?;
                ensure_canonical_stays_within_root(&root_canonical, &canonical)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to inspect storage parent component '{}': {error}",
                        existing.display()
                    ),
                )
                .with_cause(error));
            }
        }
    }
    Ok(())
}

async fn canonicalize_root(root: &Path) -> AppResult<PathBuf> {
    tokio::fs::canonicalize(root).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to canonicalize storage root '{}': {error}",
                root.display()
            ),
        )
        .with_cause(error)
    })
}

fn ensure_canonical_stays_within_root(root: &Path, canonical: &Path) -> AppResult<PathBuf> {
    if canonical.starts_with(root) {
        Ok(canonical.to_path_buf())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "storage path '{}' escapes configured root '{}'",
                canonical.display(),
                root.display()
            ),
        ))
    }
}

/// Build a sanitized temporary path next to a local storage target.
pub(super) fn storage_temp_path(target: &Path) -> PathBuf {
    let filename = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    rskit_fs::sibling_temp_path(target, filename, ".rskit-tmp")
}

/// Replace a target path with a completed sibling temporary file.
pub(super) async fn replace_with_temp(temp_path: &Path, target: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        let _ = async_io::file::remove_if_exists(target).await?;
    }
    tokio::fs::rename(temp_path, target).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to replace '{}' with '{}': {error}",
                target.display(),
                temp_path.display()
            ),
        )
        .with_cause(error)
    })
}

/// Build the canonical not-found error for a storage key.
pub(super) fn file_not_found_error(key: &str) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("file not found: {key}"))
}

/// Build the canonical not-found error for a storage key with an underlying cause.
pub(super) fn file_not_found_error_with_cause(key: &str, cause: AppError) -> AppError {
    file_not_found_error(key).with_cause(cause)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn canonicalize_and_parent_helpers_report_confined_error_paths() {
        let root = tempfile::tempdir().unwrap();
        let missing_root = root.path().join("missing-root");
        let err = canonicalize_confined(&missing_root, &missing_root.join("file"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.to_string().contains("storage root"));

        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("outside.txt");
        std::fs::write(&outside_file, b"x").unwrap();
        let err = canonicalize_confined(root.path(), &outside_file)
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("escapes configured root"));

        let parent = root.path().join("file-parent");
        std::fs::write(&parent, b"x").unwrap();
        let err = ensure_target_parent_confined(root.path(), &parent.join("child.txt"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("not a directory"));
    }

    #[tokio::test]
    async fn replace_with_temp_reports_rename_errors_and_temp_names_are_siblings() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.txt");
        let temp = storage_temp_path(&target);

        assert_eq!(temp.parent(), target.parent());
        assert!(
            temp.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".rskit-tmp")
        );

        let err = replace_with_temp(&temp, &target).await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.to_string().contains("failed to replace"));
    }

    #[test]
    fn normalize_local_key_and_not_found_errors_are_stable() {
        assert_eq!(normalize_local_key("/a/b.txt").unwrap(), "a/b.txt");
        assert_eq!(file_not_found_error("missing").code(), ErrorCode::NotFound);
        let cause = AppError::new(ErrorCode::Internal, "io");
        assert_eq!(
            file_not_found_error_with_cause("missing", cause).code(),
            ErrorCode::NotFound
        );
    }
}

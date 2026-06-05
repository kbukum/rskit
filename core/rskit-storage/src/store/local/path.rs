//! Local store path normalization, confinement, and write helpers.

use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::async_io;

use super::super::prefixed_key;

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

pub(super) fn storage_temp_path(target: &Path) -> PathBuf {
    let filename = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    rskit_fs::sibling_temp_path(target, filename, ".rskit-tmp")
}

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

pub(super) fn file_not_found_error(key: &str) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("file not found: {key}"))
}

pub(super) fn file_not_found_error_with_cause(key: &str, cause: AppError) -> AppError {
    file_not_found_error(key).with_cause(cause)
}

//! Path confinement for FFmpeg local file inputs and outputs.
//!
//! All user-provided paths that become subprocess arguments pass through this
//! module before reaching `rskit-process`.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;

use crate::config::FfmpegConfig;

pub(crate) fn confine_source_path(config: &FfmpegConfig, path: &Path) -> AppResult<PathBuf> {
    match config.path_root() {
        Some(root) => rskit_fs::confine_existing_path(root, path),
        None => Ok(path.to_path_buf()),
    }
}

pub(crate) fn confine_output_path(config: &FfmpegConfig, path: &Path) -> AppResult<PathBuf> {
    match config.path_root() {
        Some(root) => rskit_fs::confine_path(root, path),
        None => Ok(path.to_path_buf()),
    }
}

pub(crate) async fn create_output_parent(config: &FfmpegConfig, output: &Path) -> AppResult<()> {
    let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    match config.path_root() {
        Some(root) => {
            let parent = rskit_fs::confine_path(root, parent)?;
            create_confined_dir_all(root, &parent).await
        }
        None => tokio::fs::create_dir_all(parent).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("create dir failed for {}: {error}", parent.display()),
            )
        }),
    }
}

pub(crate) fn resolved_source_path(
    config: &FfmpegConfig,
    source: &FileSource,
    fallback: &Path,
) -> AppResult<PathBuf> {
    match source {
        FileSource::Path(path) => confine_source_path(config, path),
        _ => Ok(fallback.to_path_buf()),
    }
}

async fn create_confined_dir_all(root: &Path, path: &Path) -> AppResult<()> {
    let root = rskit_fs::canonicalize(root)?;
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let relative = target.strip_prefix(&root).map_err(|_| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "path '{}' resolves outside confined root '{}'",
                target.display(),
                root.display()
            ),
        )
    })?;

    let mut current = root.clone();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("path component '{component:?}' is not safe"),
            ));
        };
        current.push(segment);
        create_or_verify_plain_directory(&root, &current).await?;
    }
    Ok(())
}

async fn create_or_verify_plain_directory(root: &Path, path: &Path) -> AppResult<()> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "output parent '{}' is not a plain directory",
                    path.display()
                ),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir(path).await.map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create output directory '{}': {error}",
                        path.display()
                    ),
                )
            })?;
            let metadata = tokio::fs::symlink_metadata(path).await.map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to inspect output directory '{}': {error}",
                        path.display()
                    ),
                )
            })?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "output parent '{}' is not a plain directory",
                        path.display()
                    ),
                ));
            }
        }
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to inspect output directory '{}': {error}",
                    path.display()
                ),
            ));
        }
    }

    let canonical = rskit_fs::canonicalize(path)?;
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "output parent '{}' resolves outside confined root '{}'",
                path.display(),
                root.display()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_output_parent_rejects_symlink_escape() {
        let root = rskit_storage::TempDir::new().unwrap();
        let outside = rskit_storage::TempDir::new().unwrap();
        let link = root.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), &link).unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());

        let error = create_output_parent(&config, &link.join("out.mp4"))
            .await
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn create_output_parent_creates_nested_dirs_under_root() {
        let root = rskit_storage::TempDir::new().unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());
        let output = root.path().join("a/b/out.mp4");

        create_output_parent(&config, &output).await.unwrap();

        assert!(root.path().join("a/b").is_dir());
    }
}

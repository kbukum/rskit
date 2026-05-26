//! Permission and capability helpers.
#![allow(clippy::needless_pass_by_value)]

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::temp::sibling_temp_path;

/// Return true when filesystem metadata marks the path read-only.
pub async fn is_readonly(path: &Path) -> AppResult<bool> {
    permissions(path)
        .await
        .map(|permissions| permissions.readonly())
}

/// Read platform permissions for a path.
pub async fn permissions(path: &Path) -> AppResult<std::fs::Permissions> {
    tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.permissions())
        .map_err(|error| read_permissions_error(path, error))
}

/// Set or clear the portable read-only flag for a path.
pub async fn set_readonly(path: &Path, readonly: bool) -> AppResult<()> {
    let mut permissions = permissions(path).await?;
    permissions.set_readonly(readonly);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(|error| set_permissions_error(path, error))
}

/// Return true when the current process can open the path for reading.
pub async fn can_read(path: &Path) -> AppResult<bool> {
    can_read_from_open(path, tokio::fs::File::open(path).await)
}

fn can_read_from_open(path: &Path, result: std::io::Result<tokio::fs::File>) -> AppResult<bool> {
    match result {
        Ok(_) => Ok(true),
        Err(error) if is_permission_denied(&error) || is_not_found(&error) => Ok(false),
        Err(error) => Err(check_read_access_error(path, error)),
    }
}

/// Return true when the current process can write to the file or directory.
pub async fn can_write(path: &Path) -> AppResult<bool> {
    if tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        return can_write_dir(path).await;
    }

    can_write_from_open(
        path,
        tokio::fs::OpenOptions::new().write(true).open(path).await,
    )
}

fn can_write_from_open(path: &Path, result: std::io::Result<tokio::fs::File>) -> AppResult<bool> {
    match result {
        Ok(_) => Ok(true),
        Err(error) if is_permission_denied(&error) || is_not_found(&error) => Ok(false),
        Err(error) => Err(check_write_access_error(path, error)),
    }
}

async fn can_write_dir(path: &Path) -> AppResult<bool> {
    let probe = sibling_temp_path(&path.join(".probe"), "rskit-fs-permission", ".tmp");
    can_write_dir_from_open(
        path,
        &probe,
        tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .await,
    )
    .await
}

async fn can_write_dir_from_open(
    path: &Path,
    probe: &Path,
    result: std::io::Result<tokio::fs::File>,
) -> AppResult<bool> {
    match result {
        Ok(_) => {
            let _ = tokio::fs::remove_file(&probe).await;
            Ok(true)
        }
        Err(error) if is_permission_denied(&error) || is_not_found(&error) => Ok(false),
        Err(error) => Err(check_dir_write_access_error(path, error)),
    }
}

/// Return true when a path has any executable bit set on Unix.
#[cfg(unix)]
pub async fn is_executable(path: &Path) -> AppResult<bool> {
    mode(path).await.map(|mode| mode & 0o111 != 0)
}

/// Read Unix permission bits.
#[cfg(unix)]
pub async fn mode(path: &Path) -> AppResult<u32> {
    use std::os::unix::fs::PermissionsExt;

    permissions(path)
        .await
        .map(|permissions| permissions.mode())
}

/// Set Unix permission bits.
#[cfg(unix)]
pub async fn set_mode(path: &Path, mode: u32) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .await
        .map_err(|error| set_mode_error(path, error))
}

fn read_permissions_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to read permissions for '{}': {error}",
            path.display()
        ),
    )
}

fn set_permissions_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to set permissions for '{}': {error}",
            path.display()
        ),
    )
}

fn check_read_access_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to check read access for '{}': {error}",
            path.display()
        ),
    )
}

fn check_write_access_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to check write access for '{}': {error}",
            path.display()
        ),
    )
}

fn check_dir_write_access_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!(
            "failed to check directory write access for '{}': {error}",
            path.display()
        ),
    )
}

#[cfg(unix)]
fn set_mode_error(path: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to set mode for '{}': {error}", path.display()),
    )
}

fn is_permission_denied(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

fn is_not_found(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

#[cfg(test)]
mod tests {
    use super::{
        can_read, can_read_from_open, can_write, can_write_dir_from_open, can_write_from_open,
        check_dir_write_access_error, check_read_access_error, check_write_access_error,
        is_readonly, read_permissions_error, set_permissions_error, set_readonly,
    };
    use crate::{TempDir, file};

    #[tokio::test]
    async fn checks_read_and_write_access() {
        let dir = TempDir::new().unwrap();
        let path = dir.child("file.txt").unwrap();
        file::write(&path, b"content").await.unwrap();

        assert!(can_read(&path).await.unwrap());
        assert!(can_write(&path).await.unwrap());
        assert!(!is_readonly(&path).await.unwrap());
    }

    #[tokio::test]
    async fn read_and_write_access_return_false_for_missing_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();

        assert!(!can_read(&missing).await.unwrap());
        assert!(!can_write(&missing).await.unwrap());
    }

    #[tokio::test]
    async fn checks_directory_write_access() {
        let dir = TempDir::new().unwrap();

        assert!(can_write(dir.path()).await.unwrap());
    }

    #[tokio::test]
    async fn toggles_readonly_flag() {
        let dir = TempDir::new().unwrap();
        let path = dir.child("file.txt").unwrap();
        file::write(&path, b"content").await.unwrap();

        set_readonly(&path, true).await.unwrap();
        assert!(is_readonly(&path).await.unwrap());
        set_readonly(&path, false).await.unwrap();
        assert!(!is_readonly(&path).await.unwrap());
    }

    #[tokio::test]
    async fn permission_errors_are_reported() {
        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();

        assert!(is_readonly(&missing).await.is_err());
        assert!(set_readonly(&missing, true).await.is_err());
    }

    #[test]
    fn permission_error_builders_include_context() {
        let path = std::path::Path::new("file.txt");
        let err = || std::io::Error::other("boom");

        assert!(
            read_permissions_error(path, err())
                .to_string()
                .contains("read permissions")
        );
        assert!(
            set_permissions_error(path, err())
                .to_string()
                .contains("set permissions")
        );
        assert!(
            check_read_access_error(path, err())
                .to_string()
                .contains("read access")
        );
        assert!(
            check_write_access_error(path, err())
                .to_string()
                .contains("write access")
        );
        assert!(
            check_dir_write_access_error(path, err())
                .to_string()
                .contains("directory write access")
        );
        assert!(can_read_from_open(path, Err(err())).is_err());
        assert!(can_write_from_open(path, Err(err())).is_err());
    }

    #[tokio::test]
    async fn directory_write_result_mapping_reports_errors() {
        let dir = TempDir::new().unwrap();
        let probe = dir.child("probe.tmp").unwrap();

        assert!(
            can_write_dir_from_open(dir.path(), &probe, Err(std::io::Error::other("boom")))
                .await
                .is_err()
        );
        assert!(
            !can_write_dir_from_open(
                dir.path(),
                &probe,
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "denied"
                )),
            )
            .await
            .unwrap()
        );
        assert!(
            !can_write_dir_from_open(
                dir.path(),
                &probe,
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing")),
            )
            .await
            .unwrap()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_mode_helpers_work() {
        use super::{is_executable, mode, set_mode};

        let dir = TempDir::new().unwrap();
        let path = dir.child("script.sh").unwrap();
        file::write(&path, b"#!/bin/sh\n").await.unwrap();

        set_mode(&path, 0o755).await.unwrap();
        assert_eq!(mode(&path).await.unwrap() & 0o777, 0o755);
        assert!(is_executable(&path).await.unwrap());

        set_mode(&path, 0o644).await.unwrap();
        assert!(!is_executable(&path).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_mode_errors_are_reported() {
        use super::{is_executable, mode, set_mode, set_mode_error};

        let dir = TempDir::new().unwrap();
        let missing = dir.child("missing.txt").unwrap();

        assert!(mode(&missing).await.is_err());
        assert!(is_executable(&missing).await.is_err());
        assert!(set_mode(&missing, 0o644).await.is_err());
        assert!(
            set_mode_error(&missing, std::io::Error::other("boom"))
                .to_string()
                .contains("set mode")
        );
    }
}

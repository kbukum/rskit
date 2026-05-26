//! Permission and capability helpers.

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
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to read permissions for '{}': {error}",
                    path.display()
                ),
            )
        })
}

/// Set or clear the portable read-only flag for a path.
pub async fn set_readonly(path: &Path, readonly: bool) -> AppResult<()> {
    let mut permissions = permissions(path).await?;
    permissions.set_readonly(readonly);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to set permissions for '{}': {error}",
                    path.display()
                ),
            )
        })
}

/// Return true when the current process can open the path for reading.
pub async fn can_read(path: &Path) -> AppResult<bool> {
    match tokio::fs::File::open(path).await {
        Ok(_) => Ok(true),
        Err(error) if is_permission_denied(&error) || is_not_found(&error) => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to check read access for '{}': {error}",
                path.display()
            ),
        )),
    }
}

/// Return true when the current process can write to the file or directory.
pub async fn can_write(path: &Path) -> AppResult<bool> {
    if tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return can_write_dir(path).await;
    }

    match tokio::fs::OpenOptions::new().write(true).open(path).await {
        Ok(_) => Ok(true),
        Err(error) if is_permission_denied(&error) || is_not_found(&error) => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to check write access for '{}': {error}",
                path.display()
            ),
        )),
    }
}

async fn can_write_dir(path: &Path) -> AppResult<bool> {
    let probe = sibling_temp_path(&path.join(".probe"), "rskit-fs-permission", ".tmp");
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .await
    {
        Ok(_) => {
            let _ = tokio::fs::remove_file(&probe).await;
            Ok(true)
        }
        Err(error) if is_permission_denied(&error) => Ok(false),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to check directory write access for '{}': {error}",
                path.display()
            ),
        )),
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
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to set mode for '{}': {error}", path.display()),
            )
        })
}

fn is_permission_denied(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

fn is_not_found(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

#[cfg(test)]
mod tests {
    use super::{can_read, can_write, is_readonly, set_readonly};
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
    async fn toggles_readonly_flag() {
        let dir = TempDir::new().unwrap();
        let path = dir.child("file.txt").unwrap();
        file::write(&path, b"content").await.unwrap();

        set_readonly(&path, true).await.unwrap();
        assert!(is_readonly(&path).await.unwrap());
        set_readonly(&path, false).await.unwrap();
        assert!(!is_readonly(&path).await.unwrap());
    }
}

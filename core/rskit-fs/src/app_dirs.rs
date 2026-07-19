//! Platform application directory helpers.

use std::{ffi::OsString, path::PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Return the platform cache directory for `app_name`.
///
/// The returned directory is not created. Callers own creation and cleanup policy.
///
/// Platform behavior:
/// - macOS: `$HOME/Library/Caches/<app_name>`
/// - Windows: `%LOCALAPPDATA%\<app_name>\Cache`
/// - Unix: `$XDG_CACHE_HOME/<app_name>` or `$HOME/.cache/<app_name>`
pub fn app_cache_dir(app_name: &str) -> AppResult<PathBuf> {
    app_cache_dir_from_env(app_name, |key| std::env::var_os(key))
}

fn app_cache_dir_from_env(
    app_name: &str,
    env: impl Fn(&str) -> Option<OsString>,
) -> AppResult<PathBuf> {
    validate_app_name(app_name)?;
    let base = platform_cache_base(env)?;
    #[cfg(windows)]
    return Ok(base.join(app_name).join("Cache"));
    #[cfg(not(windows))]
    Ok(base.join(app_name))
}

fn validate_app_name(app_name: &str) -> AppResult<()> {
    if app_name.is_empty() || app_name.len() > 64 {
        return Err(invalid_app_name_error());
    }
    if !app_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid_app_name_error());
    }
    Ok(())
}

fn invalid_app_name_error() -> AppError {
    AppError::invalid_input(
        "app_name",
        "app name must be 1-64 ASCII letters, digits, '-' or '_'",
    )
}

#[cfg(target_os = "macos")]
fn platform_cache_base(env: impl Fn(&str) -> Option<OsString>) -> AppResult<PathBuf> {
    env_absolute_path("HOME", env).map(|home| home.join("Library").join("Caches"))
}

#[cfg(windows)]
fn platform_cache_base(env: impl Fn(&str) -> Option<OsString>) -> AppResult<PathBuf> {
    if let Some(local_app_data) = env_path_if_absolute("LOCALAPPDATA", &env)? {
        return Ok(local_app_data);
    }
    env_absolute_path("USERPROFILE", env).map(|home| home.join("AppData").join("Local"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_cache_base(env: impl Fn(&str) -> Option<OsString>) -> AppResult<PathBuf> {
    if let Some(xdg_cache_home) = env_path_if_absolute("XDG_CACHE_HOME", &env)? {
        return Ok(xdg_cache_home);
    }
    env_absolute_path("HOME", env).map(|home| home.join(".cache"))
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn env_path_if_absolute(
    key: &str,
    env: &impl Fn(&str) -> Option<OsString>,
) -> AppResult<Option<PathBuf>> {
    let Some(value) = env(key) else {
        return Ok(None);
    };
    if value.as_os_str().is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{key} must be an absolute path when set"),
        ));
    }
    Ok(Some(path))
}

fn env_absolute_path(key: &str, env: impl Fn(&str) -> Option<OsString>) -> AppResult<PathBuf> {
    let value = env(key).ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("{key} is required to resolve the application cache directory"),
        )
    })?;
    if value.as_os_str().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{key} cannot be empty"),
        ));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{key} must be an absolute path"),
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, ffi::OsString, path::Path};

    use super::{app_cache_dir_from_env, validate_app_name};

    #[test]
    fn rejects_invalid_app_names() {
        for name in ["", ".", "..", "bad/name", "bad name", "bad.name"] {
            assert!(validate_app_name(name).is_err(), "{name}");
        }
        assert!(validate_app_name("toven").is_ok());
        assert!(validate_app_name("my-app_1").is_ok());
    }

    #[test]
    fn resolves_platform_cache_dir_from_env() {
        let mut env = BTreeMap::new();
        #[cfg(target_os = "macos")]
        {
            env.insert("HOME", OsString::from("/Users/alice"));
            assert_eq!(
                app_cache_dir_from_env("toven", |key| env.get(key).cloned()).unwrap(),
                Path::new("/Users/alice/Library/Caches/toven")
            );
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            env.insert("HOME", OsString::from("/home/alice"));
            assert_eq!(
                app_cache_dir_from_env("toven", |key| env.get(key).cloned()).unwrap(),
                Path::new("/home/alice/.cache/toven")
            );
            env.insert("XDG_CACHE_HOME", OsString::from("/var/cache/alice"));
            assert_eq!(
                app_cache_dir_from_env("toven", |key| env.get(key).cloned()).unwrap(),
                Path::new("/var/cache/alice/toven")
            );
        }
        #[cfg(windows)]
        {
            env.insert("USERPROFILE", OsString::from(r"C:\Users\alice"));
            assert_eq!(
                app_cache_dir_from_env("toven", |key| env.get(key).cloned()).unwrap(),
                Path::new(r"C:\Users\alice\AppData\Local\toven").join("Cache")
            );
            env.insert(
                "LOCALAPPDATA",
                OsString::from(r"C:\Users\alice\AppData\Local"),
            );
            assert_eq!(
                app_cache_dir_from_env("toven", |key| env.get(key).cloned()).unwrap(),
                Path::new(r"C:\Users\alice\AppData\Local\toven").join("Cache")
            );
        }
    }

    #[test]
    fn rejects_relative_platform_cache_base() {
        let mut env = BTreeMap::new();
        #[cfg(target_os = "macos")]
        env.insert("HOME", OsString::from("relative"));
        #[cfg(all(unix, not(target_os = "macos")))]
        env.insert("XDG_CACHE_HOME", OsString::from("relative"));
        #[cfg(windows)]
        env.insert("LOCALAPPDATA", OsString::from("relative"));

        assert!(app_cache_dir_from_env("toven", |key| env.get(key).cloned()).is_err());
    }

    #[test]
    fn public_cache_dir_and_missing_or_empty_home_errors_are_mapped() {
        let mut env = BTreeMap::new();
        assert!(app_cache_dir_from_env("toven", |key| env.get(key).cloned()).is_err());

        #[cfg(target_os = "macos")]
        env.insert("HOME", OsString::from(""));
        #[cfg(all(unix, not(target_os = "macos")))]
        env.insert("HOME", OsString::from(""));
        #[cfg(windows)]
        env.insert("USERPROFILE", OsString::from(""));

        assert!(app_cache_dir_from_env("toven", |key| env.get(key).cloned()).is_err());
        let _ = super::app_cache_dir("toven");
    }
}

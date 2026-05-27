use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};

use super::env::{normalize_env_key, parse_env_value};
use super::source::ConfigSource;

/// Profile environment-file selection.
#[derive(Debug, Clone)]
pub enum Profile {
    /// Use the given profile name.
    Name(String),
    /// Read the profile name from `ENVIRONMENT`.
    FromEnvironment,
}

impl Profile {
    pub(crate) fn resolve(&self) -> AppResult<Cow<'_, str>> {
        let profile_name = match self {
            Profile::Name(name) => Cow::Borrowed(name.as_str()),
            Profile::FromEnvironment => std::env::var("ENVIRONMENT")
                .map_err(|_| {
                    AppError::invalid_input(
                        "config",
                        "ENVIRONMENT must be set when profile is resolved from the environment",
                    )
                })?
                .trim()
                .to_owned()
                .into(),
        };

        if profile_name.trim().is_empty() {
            return Err(AppError::invalid_input(
                "config",
                "profile name cannot be empty",
            ));
        }

        Ok(profile_name)
    }
}

/// Dotenv file source.
#[derive(Debug, Clone)]
pub struct DotenvFileSource {
    path: PathBuf,
    env_prefix: String,
    label: &'static str,
    ignore_malformed: bool,
}

impl DotenvFileSource {
    /// Create a fail-closed dotenv file source.
    pub fn required(path: impl Into<PathBuf>, env_prefix: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            env_prefix: env_prefix.into(),
            label: "env file",
            ignore_malformed: false,
        }
    }

    pub(crate) fn auto_discovered(path: impl Into<PathBuf>, env_prefix: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            env_prefix: env_prefix.into(),
            label: ".env file",
            ignore_malformed: true,
        }
    }

    pub(crate) fn profile(path: impl Into<PathBuf>, env_prefix: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            env_prefix: env_prefix.into(),
            label: "profile env file",
            ignore_malformed: false,
        }
    }
}

impl ConfigSource for DotenvFileSource {
    fn collect(&self) -> AppResult<config::Config> {
        match dotenv_config_from_path(&self.path, &self.env_prefix, self.label) {
            Ok(config) => Ok(config),
            Err(err) if self.ignore_malformed => {
                tracing::warn!(error = %err, "failed to load auto-discovered .env file");
                config::Config::builder()
                    .build()
                    .map_err(|e| AppError::invalid_input("config", e.to_string()))
            }
            Err(err) => Err(err),
        }
    }
}

pub(crate) fn find_profile_env_file(profile: &str) -> Option<PathBuf> {
    [
        format!("./config/profiles/{profile}.env"),
        format!("../config/profiles/{profile}.env"),
        format!("../../config/profiles/{profile}.env"),
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

pub(crate) fn find_default_env_file() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".env");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn dotenv_config_from_path(path: &Path, prefix: &str, label: &str) -> AppResult<config::Config> {
    let iter = dotenvy::from_path_iter(path).map_err(|e| {
        AppError::invalid_input(
            "config",
            format!("failed to load {label} '{}': {e}", path.display()),
        )
    })?;

    let mut builder = config::Config::builder();
    for item in iter {
        let (key, value) = item.map_err(|e| {
            AppError::invalid_input(
                "config",
                format!("failed to parse {label} '{}': {e}", path.display()),
            )
        })?;
        if let Some(key) = normalize_env_key(prefix, &key) {
            builder = builder
                .set_override(key, parse_env_value(value))
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }
    }

    builder
        .build()
        .map_err(|e| AppError::invalid_input("config", e.to_string()))
}

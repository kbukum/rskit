use std::borrow::Cow;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};

use crate::AppConfig;

/// Loads typed configuration from layered sources.
///
/// Resolution order (last wins):
/// 1. Programmatic defaults from [`ConfigLoader::with_default`]
/// 2. `config.toml` / `config/config.toml` or the file from [`ConfigLoader::with_config_file`]
/// 3. Profile env file `config/profiles/{profile}.env` (via [`ConfigLoader::with_profile`])
/// 4. `.env` file via dotenvy
/// 5. Environment variables with `__` separator, no prefix by default
///    (`DATABASE__HOST` → `database.host`).
///    A prefix can be set with [`ConfigLoader::with_env_prefix`].
/// 6. Programmatic overrides from [`ConfigLoader::with_override`]
#[derive(Debug, Default)]
pub struct ConfigLoader {
    defaults: Vec<(String, config::Value)>,
    config_file: Option<PathBuf>,
    env_file: Option<PathBuf>,
    env_prefix: String,
    profile: Option<Profile>,
    overrides: Vec<(String, config::Value)>,
}

#[derive(Debug)]
enum Profile {
    Name(String),
    FromEnvironment,
}

impl ConfigLoader {
    /// Create a new [`ConfigLoader`] with default settings (no env prefix).
    pub fn new() -> Self {
        Self {
            defaults: Vec::new(),
            config_file: None,
            env_file: None,
            env_prefix: String::new(),
            profile: None,
            overrides: Vec::new(),
        }
    }

    /// Set a programmatic default value.
    ///
    /// Defaults are loaded before files and environment variables, so every
    /// external source can override them.
    #[must_use]
    pub fn with_default(mut self, key: impl Into<String>, value: impl Into<config::Value>) -> Self {
        self.defaults.push((key.into(), value.into()));
        self
    }

    /// Explicitly set the TOML config file path.
    #[must_use]
    pub fn with_config_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_file = Some(path.into());
        self
    }

    /// Explicitly set the `.env` file path.
    #[must_use]
    pub fn with_env_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.env_file = Some(path.into());
        self
    }

    /// Override the env-var prefix (default: `""`).
    /// Separator is always `"__"`.
    #[must_use]
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = prefix.into();
        self
    }

    /// Set the configuration profile (e.g., "development", "docker", "staging").
    ///
    /// Loads `config/profiles/{profile}.env` before the main `.env` file. If an
    /// empty string is passed, the profile name is read from the `ENVIRONMENT`
    /// environment variable during [`ConfigLoader::load`]. Missing profile names
    /// or missing profile files are treated as configuration errors.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        let p = profile.into();
        self.profile = Some(if p.is_empty() {
            Profile::FromEnvironment
        } else {
            Profile::Name(p)
        });
        self
    }

    /// Set a programmatic override value.
    ///
    /// Overrides are loaded after files and environment variables, so they have
    /// the highest precedence and cannot be replaced by other sources.
    #[must_use]
    pub fn with_override(
        mut self,
        key: impl Into<String>,
        value: impl Into<config::Value>,
    ) -> Self {
        self.overrides.push((key.into(), value.into()));
        self
    }

    /// Load and deserialize configuration into `T`.
    pub fn load<T: AppConfig>(&self) -> AppResult<T> {
        let dotenv_sources = self.dotenv_sources()?;

        let mut builder = config::Config::builder();

        for (key, value) in &self.defaults {
            builder = builder
                .set_default(key, value.clone())
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }

        // TOML file
        if let Some(path) = &self.config_file {
            builder = builder.add_source(config::File::from(path.as_path()).required(false));
        } else {
            // Auto-discover common locations
            for candidate in &["config.toml", "config/config.toml"] {
                builder = builder.add_source(
                    config::File::with_name(candidate)
                        .required(false)
                        .format(config::FileFormat::Toml),
                );
            }
        }

        for source in dotenv_sources {
            builder = builder.add_source(source);
        }

        // Environment variables
        let env_source = if self.env_prefix.is_empty() {
            config::Environment::default()
                .separator("__")
                .try_parsing(true)
        } else {
            config::Environment::with_prefix(&self.env_prefix)
                .separator("__")
                .try_parsing(true)
        };
        builder = builder.add_source(env_source);

        for (key, value) in &self.overrides {
            builder = builder
                .set_override(key, value.clone())
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }

        let raw = builder
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

        let mut cfg: T = raw
            .try_deserialize()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

        cfg.apply_defaults();

        cfg.validate()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

        Ok(cfg)
    }

    fn dotenv_sources(&self) -> AppResult<Vec<config::Config>> {
        let mut sources = Vec::new();

        if let Some(profile) = &self.profile {
            let profile_name = match profile {
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
            let path = find_profile_env_file(profile_name.as_ref()).ok_or_else(|| {
                AppError::invalid_input(
                    "config",
                    format!("profile env file not found for profile '{profile_name}'"),
                )
            })?;
            sources.push(self.dotenv_source_from_path(&path, "profile env file")?);
        }

        if let Some(path) = &self.env_file {
            sources.push(self.dotenv_source_from_path(path, "env file")?);
        } else if let Some(path) = find_default_env_file() {
            match self.dotenv_source_from_path(&path, ".env file") {
                Ok(source) => sources.push(source),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to load auto-discovered .env file")
                }
            }
        }

        Ok(sources)
    }

    fn dotenv_source_from_path(&self, path: &Path, label: &str) -> AppResult<config::Config> {
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
            if let Some(key) = normalize_env_key(&self.env_prefix, &key) {
                builder = builder
                    .set_override(key, parse_env_value(value))
                    .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
            }
        }

        builder
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))
    }
}

fn find_profile_env_file(profile: &str) -> Option<PathBuf> {
    [
        format!("./config/profiles/{profile}.env"),
        format!("../config/profiles/{profile}.env"),
        format!("../../config/profiles/{profile}.env"),
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

fn find_default_env_file() -> Option<PathBuf> {
    find_upwards_env_file()
}

fn find_upwards_env_file() -> Option<PathBuf> {
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

fn normalize_env_key(prefix: &str, key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    if prefix.is_empty() {
        return Some(key.replace("__", "."));
    }

    let prefix = prefix.to_ascii_lowercase();
    key.strip_prefix(&format!("{prefix}__"))
        .map(|stripped| stripped.replace("__", "."))
}

fn parse_env_value(value: String) -> config::Value {
    if let Ok(value) = value.parse::<bool>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<i64>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<f64>() {
        return value.into();
    }
    value.into()
}

/// Convenience function: create a default [`ConfigLoader`] and call [`ConfigLoader::load`].
pub fn load_config<T: AppConfig>() -> AppResult<T> {
    ConfigLoader::new().load()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppConfig, ServiceConfig};
    use rskit_validation::Validate;
    use serde::Deserialize;

    // Serialise env-mutating tests — parallel tests share the same process env.
    static ENV_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[derive(Debug, Deserialize, Validate)]
    struct TestConfig {
        #[serde(flatten)]
        service: ServiceConfig,
        #[serde(default = "default_app_port")]
        app_port: u16,
    }

    fn default_app_port() -> u16 {
        8080
    }

    impl AppConfig for TestConfig {
        fn apply_defaults(&mut self) {}
        fn service_config(&self) -> &ServiceConfig {
            &self.service
        }
    }

    #[test]
    fn loads_defaults_with_no_file() {
        let _guard = ENV_LOCK.lock();
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `parking_lot::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe {
            std::env::set_var("APP_PORT", "8080");
            std::env::set_var("ADDRESS", "127.0.0.1");
            std::env::set_var("PORT", "50051");
        };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.app_port, 8080);
        assert_eq!(cfg.service.address, "127.0.0.1");
        assert_eq!(cfg.service.port, 50051);
        unsafe {
            std::env::remove_var("APP_PORT");
            std::env::remove_var("ADDRESS");
            std::env::remove_var("PORT");
        };
    }

    #[test]
    fn env_prefix_override() {
        let _guard = ENV_LOCK.lock();
        // PORT=9090 should override the default (no prefix by default).
        // SAFETY: `std::env::set_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `parking_lot::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe {
            std::env::set_var("APP_PORT", "9090");
            std::env::set_var("ADDRESS", "127.0.0.1");
            std::env::set_var("PORT", "50051");
        };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.app_port, 9090);
        assert_eq!(cfg.service.address, "127.0.0.1");
        assert_eq!(cfg.service.port, 50051);
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` for the duration of this test, serializing all env mutations.
        unsafe {
            std::env::remove_var("APP_PORT");
            std::env::remove_var("ADDRESS");
            std::env::remove_var("PORT");
        };
    }

    #[test]
    fn custom_prefix_is_respected() {
        let _guard = ENV_LOCK.lock();
        // SAFETY: `std::env::set_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `parking_lot::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe {
            std::env::set_var("SVC__APP_PORT", "7777");
            std::env::set_var("SVC__ADDRESS", "127.0.0.1");
            std::env::set_var("SVC__PORT", "50051");
        };
        let cfg: TestConfig = ConfigLoader::new()
            .with_env_prefix("SVC")
            .load()
            .expect("should load");
        assert_eq!(cfg.app_port, 7777);
        assert_eq!(cfg.service.address, "127.0.0.1");
        assert_eq!(cfg.service.port, 50051);
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` for the duration of this test, serializing all env mutations.
        unsafe {
            std::env::remove_var("SVC__APP_PORT");
            std::env::remove_var("SVC__ADDRESS");
            std::env::remove_var("SVC__PORT");
        };
    }
}

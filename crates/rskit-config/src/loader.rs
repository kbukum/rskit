use std::path::PathBuf;

use rskit_errors::{AppError, AppResult};

use crate::AppConfig;

/// Loads typed configuration from layered sources.
///
/// Resolution order (last wins):
/// 1. `config.toml` / `config/{service}.toml` (optional)
/// 2. Profile env file `config/profiles/{profile}.env` (optional, via [`ConfigLoader::with_profile`])
/// 3. `.env` file via dotenvy (optional)
/// 4. Environment variables with `__` separator, no prefix by default
///    (`DATABASE__HOST` → `database.host`).
///    A prefix can be set with [`ConfigLoader::with_env_prefix`].
#[derive(Debug, Default)]
pub struct ConfigLoader {
    config_file: Option<PathBuf>,
    env_file: Option<PathBuf>,
    env_prefix: String,
    profile: Option<String>,
}

impl ConfigLoader {
    /// Create a new [`ConfigLoader`] with default settings (no env prefix).
    pub fn new() -> Self {
        Self {
            config_file: None,
            env_file: None,
            env_prefix: String::new(),
            profile: None,
        }
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
    /// Loads `config/profiles/{profile}.env` before the main `.env` file.
    /// If profile is `None`, reads from the `ENVIRONMENT` env var.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        let p = profile.into();
        self.profile = Some(if p.is_empty() {
            std::env::var("ENVIRONMENT").unwrap_or_default()
        } else {
            p
        });
        self
    }

    /// Load and deserialize configuration into `T`.
    pub fn load<T: AppConfig>(&self) -> AppResult<T> {
        // Step 1: load .env (if present) so env vars are available to config-rs
        self.load_env_file();

        // Step 2: build the config-rs source chain
        let mut builder = config::Config::builder();

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

    fn load_env_file(&self) {
        // 1. Load profile env file first (if profile is set)
        if let Some(profile) = &self.profile
            && !profile.is_empty()
        {
            let profile_paths = [
                format!("./config/profiles/{profile}.env"),
                format!("../config/profiles/{profile}.env"),
                format!("../../config/profiles/{profile}.env"),
            ];
            for path in &profile_paths {
                if std::path::Path::new(path).exists() {
                    let _ = dotenvy::from_path(path);
                    break;
                }
            }
        }

        // 2. Load main .env file (existing behavior)
        if let Some(path) = &self.env_file {
            let _ = dotenvy::from_path(path);
        } else {
            // Try the default `.env` in the current directory — silently ignore absence
            let _ = dotenvy::dotenv();
        }
    }
}

/// Convenience function: create a default [`ConfigLoader`] and call [`ConfigLoader::load`].
pub fn load_config<T: AppConfig>() -> AppResult<T> {
    ConfigLoader::new().load()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppConfig, ServiceConfig};
    use serde::Deserialize;
    use validator::Validate;

    // Serialise env-mutating tests — parallel tests share the same process env.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[derive(Debug, Deserialize, Validate)]
    struct TestConfig {
        #[serde(flatten)]
        service: ServiceConfig,
        #[serde(default = "default_port")]
        port: u16,
    }

    fn default_port() -> u16 {
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
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `std::sync::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe { std::env::remove_var("PORT") };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.port, 8080);
    }

    #[test]
    fn env_prefix_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        // PORT=9090 should override the default (no prefix by default).
        // SAFETY: `std::env::set_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `std::sync::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe { std::env::set_var("PORT", "9090") };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.port, 9090);
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` for the duration of this test, serializing all env mutations.
        unsafe { std::env::remove_var("PORT") };
    }

    #[test]
    fn custom_prefix_is_respected() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: `std::env::set_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` (a `std::sync::Mutex`) for the duration of this test,
        // serializing all environment variable mutations. No other test runs concurrently.
        unsafe { std::env::set_var("SVC__PORT", "7777") };
        let cfg: TestConfig = ConfigLoader::new()
            .with_env_prefix("SVC")
            .load()
            .expect("should load");
        assert_eq!(cfg.port, 7777);
        // SAFETY: `std::env::remove_var` is unsafe because concurrent calls can cause data races.
        // We hold `ENV_LOCK` for the duration of this test, serializing all env mutations.
        unsafe { std::env::remove_var("SVC__PORT") };
    }
}

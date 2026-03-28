use std::path::PathBuf;

use rskit_errors::{AppError, AppResult};

use crate::AppConfig;

/// Loads typed configuration from layered sources.
///
/// Resolution order (last wins):
/// 1. `config.toml` / `config/{service}.toml` (optional)
/// 2. `.env` file via dotenvy (optional)
/// 3. Environment variables with `APP__` prefix, `__` separator
///    (`APP__DATABASE__HOST` → `database.host`)
#[derive(Debug, Default)]
pub struct ConfigLoader {
    config_file: Option<PathBuf>,
    env_file: Option<PathBuf>,
    env_prefix: String,
}

impl ConfigLoader {
    /// Create a new [`ConfigLoader`] with default settings (prefix `"APP"`).
    pub fn new() -> Self {
        Self {
            config_file: None,
            env_file: None,
            env_prefix: "APP".to_string(),
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

    /// Override the env-var prefix (default: `"APP"`).
    /// Separator is always `"__"`.
    #[must_use]
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = prefix.into();
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
            builder = builder.add_source(
                config::File::from(path.as_path()).required(false),
            );
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

        // Environment variables: APP__DATABASE__HOST → database.host
        builder = builder.add_source(
            config::Environment::with_prefix(&self.env_prefix)
                .separator("__")
                .try_parsing(true),
        );

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
        // Ensure the override var is absent for this test.
        unsafe { std::env::remove_var("APP__PORT") };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.port, 8080);
    }

    #[test]
    fn env_prefix_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        // APP__PORT=9090 should override the default.
        // SAFETY: serialised by ENV_LOCK; no other thread mutates this var.
        unsafe { std::env::set_var("APP__PORT", "9090") };
        let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
        assert_eq!(cfg.port, 9090);
        unsafe { std::env::remove_var("APP__PORT") };
    }

    #[test]
    fn custom_prefix_is_respected() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SVC__PORT", "7777") };
        let cfg: TestConfig = ConfigLoader::new()
            .with_env_prefix("SVC")
            .load()
            .expect("should load");
        assert_eq!(cfg.port, 7777);
        unsafe { std::env::remove_var("SVC__PORT") };
    }
}

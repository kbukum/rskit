//! Adapter-oriented configuration loading.

mod decode;
mod dotenv;
mod env;
mod source;

use std::path::PathBuf;

use rskit_errors::{AppError, AppResult};
use rskit_validation::Validate;
use serde::de::DeserializeOwned;

use crate::AppConfig;

pub use dotenv::{DotenvFileSource, Profile};
pub use env::EnvironmentSource;
pub use source::{ConfigMapSource, ConfigSource, TomlFileSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoaderPolicy {
    App,
    Toml,
    Custom,
}

/// Loads typed configuration from ordered source adapters.
///
/// `ConfigLoader::app()` / `ConfigLoader::new()` keeps the service-oriented
/// policy: defaults → TOML → profile dotenv → dotenv → adapter sources → env →
/// overrides. `ConfigLoader::toml(path)` is deterministic file-only loading.
/// `ConfigLoader::custom()` only loads explicitly provided sources.
#[derive(Debug)]
pub struct ConfigLoader {
    policy: LoaderPolicy,
    defaults: Vec<(String, config::Value)>,
    config_file: Option<PathBuf>,
    env_file: Option<PathBuf>,
    env_prefix: String,
    profile: Option<Profile>,
    sources: Vec<Box<dyn ConfigSource>>,
    overrides: Vec<(String, config::Value)>,
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::app()
    }
}

impl ConfigLoader {
    /// Create an application/service loader.
    pub fn new() -> Self {
        Self::app()
    }

    /// Create an application/service loader.
    pub fn app() -> Self {
        Self {
            policy: LoaderPolicy::App,
            defaults: Vec::new(),
            config_file: None,
            env_file: None,
            env_prefix: String::new(),
            profile: None,
            sources: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Create a deterministic single-TOML loader.
    ///
    /// This policy does not read dotenv files or process environment variables.
    pub fn toml(path: impl Into<PathBuf>) -> Self {
        Self {
            policy: LoaderPolicy::Toml,
            defaults: Vec::new(),
            config_file: Some(path.into()),
            env_file: None,
            env_prefix: String::new(),
            profile: None,
            sources: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Create a loader with no implicit sources.
    pub fn custom() -> Self {
        Self {
            policy: LoaderPolicy::Custom,
            defaults: Vec::new(),
            config_file: None,
            env_file: None,
            env_prefix: String::new(),
            profile: None,
            sources: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Set a programmatic default value.
    ///
    /// Defaults are loaded before all configured sources.
    #[must_use]
    pub fn with_default(mut self, key: impl Into<String>, value: impl Into<config::Value>) -> Self {
        self.defaults.push((key.into(), value.into()));
        self
    }

    /// Explicitly set the TOML config file path for app loading.
    #[must_use]
    pub fn with_config_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_file = Some(path.into());
        self
    }

    /// Explicitly set the `.env` file path for app loading.
    #[must_use]
    pub fn with_env_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.env_file = Some(path.into());
        self
    }

    /// Override the env-var prefix for app loading.
    ///
    /// Separator is always `"__"`.
    #[must_use]
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = prefix.into();
        self
    }

    /// Set the configuration profile for app loading.
    ///
    /// Loads `config/profiles/{profile}.env` before the main `.env` file. If an
    /// empty string is passed, the profile name is read from the `ENVIRONMENT`
    /// environment variable during loading.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        let profile = profile.into();
        self.profile = Some(if profile.is_empty() {
            Profile::FromEnvironment
        } else {
            Profile::Name(profile)
        });
        self
    }

    /// Add an adapter source.
    ///
    /// In app loading, adapter sources are evaluated after files/dotenv and
    /// before process environment variables. In custom/TOML loading, adapter
    /// sources are evaluated after the primary file and before overrides.
    #[must_use]
    pub fn with_source(mut self, source: impl ConfigSource) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Set a programmatic override value.
    ///
    /// Overrides are loaded after all sources and have the highest precedence.
    #[must_use]
    pub fn with_override(
        mut self,
        key: impl Into<String>,
        value: impl Into<config::Value>,
    ) -> Self {
        self.overrides.push((key.into(), value.into()));
        self
    }

    /// Load any typed config from this loader's source policy.
    pub fn load<T>(&self) -> AppResult<T>
    where
        T: DeserializeOwned + Validate,
    {
        self.load_with(|_| {})
    }

    /// Load any typed config and apply defaults before validation.
    pub fn load_with<T>(&self, apply_defaults: impl FnOnce(&mut T)) -> AppResult<T>
    where
        T: DeserializeOwned + Validate,
    {
        decode::decode(self.collect()?, apply_defaults)
    }

    /// Load an application/service config and call [`AppConfig::apply_defaults`].
    pub fn load_app<T>(&self) -> AppResult<T>
    where
        T: AppConfig,
    {
        self.load_with(T::apply_defaults)
    }

    fn collect(&self) -> AppResult<config::Config> {
        let mut builder = config::Config::builder();

        for (key, value) in &self.defaults {
            builder = builder
                .set_default(key, value.clone())
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }

        for source in self.policy_sources()? {
            builder = builder.add_source(source.collect()?);
        }

        for source in &self.sources {
            builder = builder.add_source(source.collect()?);
        }

        if self.policy == LoaderPolicy::App {
            builder =
                builder.add_source(EnvironmentSource::with_prefix(&self.env_prefix).collect()?);
        }

        for (key, value) in &self.overrides {
            builder = builder
                .set_override(key, value.clone())
                .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
        }

        builder
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))
    }

    fn policy_sources(&self) -> AppResult<Vec<Box<dyn ConfigSource>>> {
        match self.policy {
            LoaderPolicy::App => self.app_policy_sources(),
            LoaderPolicy::Toml => {
                let path = self.config_file.as_ref().ok_or_else(|| {
                    AppError::invalid_input("config", "TOML loader requires a config file")
                })?;
                Ok(vec![Box::new(TomlFileSource::required(path.clone()))])
            }
            LoaderPolicy::Custom => Ok(Vec::new()),
        }
    }

    fn app_policy_sources(&self) -> AppResult<Vec<Box<dyn ConfigSource>>> {
        let mut sources: Vec<Box<dyn ConfigSource>> = Vec::new();

        if let Some(path) = &self.config_file {
            sources.push(Box::new(TomlFileSource::optional(path.clone())));
        } else {
            sources.push(Box::new(TomlFileSource::optional("config.toml")));
            sources.push(Box::new(TomlFileSource::optional("config/config.toml")));
        }

        if let Some(profile) = &self.profile {
            let profile_name = profile.resolve()?;
            let path = dotenv::find_profile_env_file(profile_name.as_ref()).ok_or_else(|| {
                AppError::invalid_input(
                    "config",
                    format!("profile env file not found for profile '{profile_name}'"),
                )
            })?;
            sources.push(Box::new(DotenvFileSource::profile(
                path,
                self.env_prefix.clone(),
            )));
        }

        if let Some(path) = &self.env_file {
            sources.push(Box::new(DotenvFileSource::required(
                path.clone(),
                self.env_prefix.clone(),
            )));
        } else if let Some(path) = dotenv::find_default_env_file() {
            sources.push(Box::new(DotenvFileSource::auto_discovered(
                path,
                self.env_prefix.clone(),
            )));
        }

        Ok(sources)
    }
}

/// Convenience function: create a default app loader and call [`ConfigLoader::load_app`].
pub fn load_config<T: AppConfig>() -> AppResult<T> {
    ConfigLoader::app().load_app()
}

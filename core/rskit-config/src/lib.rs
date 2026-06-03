//! Adapter-oriented configuration loading with validation.
//!
//! # Example
//!
//! ```no_run
//! use rskit_config::{AppConfig, ConfigLoader, SecretString, ServiceConfig};
//! use rskit_validation::Validate;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize, Validate)]
//! struct MyConfig {
//!     #[serde(flatten)]
//!     #[validate(nested)]
//!     service: ServiceConfig,
//!     #[validate(range(min = 1, max = 65535))]
//!     grpc_port: u16,
//!     api_token: SecretString,
//! }
//!
//! impl AppConfig for MyConfig {
//!     fn apply_defaults(&mut self) {
//!         if self.grpc_port == 0 {
//!             self.grpc_port = 50051;
//!         }
//!     }
//!
//!     fn service_config(&self) -> &ServiceConfig {
//!         &self.service
//!     }
//! }
//!
//! # fn main() -> rskit_errors::AppResult<()> {
//! let cfg: MyConfig = ConfigLoader::app()
//!     .with_default("grpc_port", 50051_i64)
//!     .with_env_prefix("MYAPP")
//!     .load_app()?;
//! assert_eq!(cfg.api_token.to_string(), "***");
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod loader;
mod normalize;
mod service;

pub use loader::{
    ConfigLoader, ConfigMapSource, ConfigSource, DotenvFileSource, EnvironmentSource, Profile,
    TomlFileSource, load_config,
};
pub use normalize::{canonicalize_root_relative_to, supported_schema};
pub use rskit_util::SecretString;
pub use service::{Environment, LogFormat, LogOutput, LoggingConfig, ServiceConfig};

/// Trait that every application config struct must implement.
///
/// Typically implemented by a struct that embeds [`ServiceConfig`] and adds
/// service-specific fields.
///
/// ```no_run
/// use rskit_config::{AppConfig, ConfigLoader, SecretString, ServiceConfig};
/// use rskit_validation::Validate;
///
/// #[derive(serde::Deserialize, Validate)]
/// struct MyConfig {
///     #[serde(flatten)]
///     #[validate(nested)]
///     service: ServiceConfig,
///     #[validate(range(min = 1, max = 65535))]
///     grpc_port: u16,
///     api_token: SecretString,
/// }
///
/// impl AppConfig for MyConfig {
///     fn apply_defaults(&mut self) {
///         if self.grpc_port == 0 { self.grpc_port = 50051; }
///     }
///     fn service_config(&self) -> &ServiceConfig { &self.service }
/// }
///
/// # fn main() -> rskit_errors::AppResult<()> {
/// let cfg: MyConfig = ConfigLoader::app().load_app()?;
/// assert_eq!(cfg.api_token.to_string(), "***");
/// # Ok(())
/// # }
/// ```
pub trait AppConfig:
    serde::de::DeserializeOwned + rskit_validation::Validate + Send + Sync + 'static
{
    /// Apply any programmatic defaults after deserialization.
    fn apply_defaults(&mut self);
    /// Return a reference to the embedded [`ServiceConfig`].
    fn service_config(&self) -> &ServiceConfig;
}

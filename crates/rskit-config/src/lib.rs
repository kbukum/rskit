//! TOML + environment-variable configuration loading with validation.

#![warn(missing_docs)]

mod loader;
mod service;

pub use loader::{load_config, ConfigLoader};
pub use service::{Environment, LogFormat, LogOutput, LoggingConfig, ServiceConfig};

/// Trait that every application config struct must implement.
///
/// Typically implemented by a struct that embeds [`ServiceConfig`] and adds
/// service-specific fields.
///
/// ```ignore
/// #[derive(serde::Deserialize, validator::Validate)]
/// struct MyConfig {
///     #[serde(flatten)]
///     service: rskit_config::ServiceConfig,
///     #[validate(range(min = 1, max = 65535))]
///     grpc_port: u16,
/// }
///
/// impl rskit_config::AppConfig for MyConfig {
///     fn apply_defaults(&mut self) {
///         if self.grpc_port == 0 { self.grpc_port = 50051; }
///     }
///     fn service_config(&self) -> &rskit_config::ServiceConfig { &self.service }
/// }
/// ```
pub trait AppConfig:
    serde::de::DeserializeOwned + validator::Validate + Send + Sync + 'static
{
    /// Apply any programmatic defaults after deserialization.
    fn apply_defaults(&mut self);
    /// Return a reference to the embedded [`ServiceConfig`].
    fn service_config(&self) -> &ServiceConfig;
}

//! Adapter-oriented configuration loading with validation.
//!
//! # Example
//!
//! The end-to-end app example below exercises the `validate`-gated App API (`AppConfig`/`ServiceConfig`/`Validate`),
//! so it only compiles when the default `validate` feature is enabled.
//!
#![cfg_attr(feature = "validate", doc = "```no_run")]
#![cfg_attr(not(feature = "validate"), doc = "```ignore")]
//! use rskit_config::{AppConfig, ConfigLoader, SecretString, ServiceConfig};
//! use rskit_validation::Validate; use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)] struct MyConfig {
//!     #[serde(flatten)]
//!     service: ServiceConfig,
//!     grpc_port: u16,
//!     api_token: SecretString,
//! }
//!
//! impl Validate for MyConfig {
//!     fn validate(&self) -> Result<(), validator::ValidationErrors> {
//!         self.service.validate()?;
//!         if self.grpc_port == 0 {
//!             let mut errors = validator::ValidationErrors::new();
//!             errors.add("grpc_port", validator::ValidationError::new("range"));
//!             return Err(errors);
//!         }
//!         Ok(())
//!     }
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

#[cfg(feature = "validate")]
mod app;
#[cfg(feature = "validate")]
mod service;
mod sink;
mod source;
mod strict;
mod typed;
#[cfg(feature = "watch")]
mod watch;

#[cfg(feature = "validate")]
pub use app::AppConfig;
pub use rskit_util::SecretString;
#[cfg(feature = "validate")]
pub use service::{Environment, LogFormat, LogOutput, LoggingConfig, ServiceConfig};
pub use sink::{ConfigSink, ConfigTable, FileConfigSink, InMemoryConfigSink};
#[cfg(feature = "validate")]
pub use source::load_config;
pub use source::{
    ConfigLoader, ConfigMapSource, ConfigSource, DotenvFileSource, EnvironmentSource, Profile,
    TomlFileSource,
};
pub use strict::{
    CompositeKey, IdentityKey, IncludeMerge, MergeIdentity, RawTable, RawValue, StrictLoader,
    deserialize_subtree, load_strict,
};
#[cfg(feature = "watch")]
pub use watch::{ConfigChange, ConfigChangeStream, ConfigWatch};

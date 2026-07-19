//! Configuration source pipeline.
//!
//! Sources are adapters ([`ConfigSource`]) collected into an ordered merge by [`ConfigLoader`].
//! The pipeline owns ordering, defaults, env merging, and overrides;
//! adapters only return collected values.

mod contract;
mod dotenv;
mod env;
mod map;
mod pipeline;
mod toml;

pub use contract::ConfigSource;
pub use dotenv::{DotenvFileSource, Profile};
pub use env::EnvironmentSource;
pub use map::ConfigMapSource;
pub use pipeline::ConfigLoader;
#[cfg(feature = "validate")]
pub use pipeline::load_config;
pub use toml::TomlFileSource;

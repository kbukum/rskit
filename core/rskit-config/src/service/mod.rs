//! Service-oriented configuration types.
//!
//! These are an opt-in convenience for long-running network services. Apps, CLIs, tools,
//! and libraries that do not need a service identity should load their own typed config through the [`crate::source`] pipeline instead.
//!
//! The logging vocabulary ([`LoggingConfig`]/[`LogFormat`]/[`LogOutput`]) is owned by `rskit-logging`
//! and re-exported here for convenience; it is plain `serde` data carrying no `tracing` dependency.

mod config;
mod environment;

pub use config::ServiceConfig;
pub use environment::Environment;
pub use rskit_logging::{LogFormat, LogOutput, LoggingConfig};

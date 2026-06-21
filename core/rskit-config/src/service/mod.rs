//! Service-oriented configuration types.
//!
//! These are an opt-in convenience for long-running network services. Apps,
//! CLIs, tools, and libraries that do not need a service identity should load
//! their own typed config through the [`crate::source`] pipeline instead.

mod config;
mod environment;
mod logging;

pub use config::ServiceConfig;
pub use environment::Environment;
pub use logging::{LogFormat, LogOutput, LoggingConfig};

//! Persistent subprocess lifecycle support.

mod cancel;
mod config;
mod error;
mod io;
mod process;
mod readiness;
mod run;

#[cfg(all(test, unix))]
mod tests;
mod types;

pub use config::{
    PersistentConfig, PersistentOutput, PersistentOutputObserver, PersistentOutputStream,
    PersistentReadiness,
};
pub use error::{PersistentStartErrorKind, persistent_start_error_kind};
pub use process::{PersistentProcess, ShutdownOutcome};
pub use run::{start_persistent_supervised, start_persistent_with_cancel};
pub use types::{PersistentRun, PersistentStartup};

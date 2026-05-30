//! Subprocess execution with process-group isolation and timeout/signal handling.
//!
//! This crate provides functionality to execute external processes with:
//! - Timeout support with configurable grace period
//! - SIGTERM → SIGKILL escalation for graceful shutdown
//! - Process group isolation to ensure child processes are properly terminated
//! - Stdout/stderr capture
//! - Environment variable control
//! - Working directory configuration
//!
//! # Example
//!
//! ```no_run
//! use rskit_process::{Command, ProcessConfig, run_with_cancel};
//! use std::time::Duration;
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let cmd = Command::new("echo")
//!     .arg("hello")
//!     .arg("world");
//!
//! let config = ProcessConfig {
//!     timeout: Some(Duration::from_secs(30)),
//!     grace_period: Duration::from_secs(5),
//!     capture_output: true,
//!     inherit_env: true,
//!     max_output_bytes: Some(rskit_process::DEFAULT_MAX_OUTPUT_BYTES),
//! };
//!
//! let result = run_with_cancel(&cmd, &config, CancellationToken::new()).await?;
//! println!("stdout: {}", result.stdout);
//! println!("exit code: {:?}", result.exit_code);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod command;
mod persistent;
mod process_group;
mod result;
mod runner;
mod signal;
mod sync;

pub use command::{Command, DEFAULT_MAX_OUTPUT_BYTES, ProcessConfig, command};
pub use persistent::{
    PersistentConfig, PersistentOutput, PersistentOutputStream, PersistentProcess,
    PersistentReadiness, PersistentRun, PersistentStartErrorKind, PersistentStartup,
    ShutdownOutcome, persistent_start_error_kind, start_persistent_with_cancel,
};
pub use process_group::{
    interrupt as interrupt_process_group, isolate as isolate_process_group,
    kill as kill_process_group, terminate as terminate_process_group,
};
pub use result::ProcessResult;
pub use runner::{OutputObserver, run_with_cancel, run_with_observer};
pub use sync::run;

/// Re-export error types
pub use rskit_errors::{AppError, AppResult, ErrorCode};

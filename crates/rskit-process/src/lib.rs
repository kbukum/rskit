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
//! use rskit_process::{Command, ProcessConfig, run};
//! use std::time::Duration;
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
//! };
//!
//! let result = run(&cmd, &config).await?;
//! println!("stdout: {}", result.stdout);
//! println!("exit code: {:?}", result.exit_code);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod command;
mod result;
mod runner;

pub use command::{Command, ProcessConfig};
pub use result::ProcessResult;
pub use runner::run;

/// Re-export error types
pub use rskit_errors::{AppError, AppResult, ErrorCode};

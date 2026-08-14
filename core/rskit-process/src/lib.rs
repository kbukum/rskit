//! Subprocess execution with explicit I/O modes, process-group isolation, and timeouts.
//!
//! This crate provides functionality to execute external processes with:
//! - Timeout support with configurable grace period
//! - SIGTERM → SIGKILL escalation for graceful shutdown
//! - Process group isolation and [`ProcessSupervisor`] tracking to ensure child processes are properly terminated on normal completion, timeout, cancellation, future drop, panic unwinding through a supervisor, and explicit supervisor shutdown
//! - Explicit captured, observed, and inherited stdio modes
//! - Environment variable control
//! - Working directory configuration
//!
//! Isolated spawns place the child in its own process group and set `kill_on_drop(true)` for Tokio children, so a dropped child handle tears the group down. Cleanup on a hard parent death (for example `SIGKILL` of the parent, which runs no destructors) is best-effort and not guaranteed on any platform; supervise long-lived children explicitly through [`ProcessSupervisor`] rather than relying on kernel parent-death signalling.
//!
//! # I/O modes
//!
//! `ProcessIo::Captured` captures stdout and stderr separately through pipes.
//! It is deterministic for non-interactive commands, but it is not a terminal
//! and cannot guarantee exact ordering between stdout and stderr.
//!
//! `ProcessIo::Observed` also uses separate pipes and supports live raw-byte
//! or line callbacks with optional capture. Line observers split deterministically on `\n`, `\r`,
//! and `\r\n`; invalid UTF-8 is reported lossily.
//!
//! `ProcessIo::Inherited` gives the child the parent stdio handles.
//! This is the right mode for normal terminal commands,
//! but it does not provide structured output capture.
//!
//! # Example
//!
//! ```no_run
//! use rskit_process::{ProcessConfig, ProcessSpec, run_with_cancel};
//! use std::time::Duration;
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let spec = ProcessSpec::new("echo")
//!     .arg("hello")
//!     .arg("world");
//!
//! let config = ProcessConfig::default().with_timeout(Some(Duration::from_secs(30)));
//!
//! let result = run_with_cancel(&spec, &config, CancellationToken::new()).await?;
//! println!("stdout: {}", result.stdout);
//! println!("exit code: {:?}", result.exit_code);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod capture;
mod command;
mod persistent;
mod process_group;
#[cfg(unix)]
mod pty;
mod result;
mod runner;
mod signal;
mod supervisor;
mod sync;
mod worker;

pub use command::{
    ArgRedaction, CapturedIo, DEFAULT_MAX_OUTPUT_BYTES, EnvPolicy, InheritedIo, InputPolicy,
    LifecyclePolicy, ObservedIo, OutputPolicy, ProcessConfig, ProcessIo, ProcessSpec, command,
};
pub use persistent::{
    PersistentConfig, PersistentOutput, PersistentOutputObserver, PersistentOutputStream,
    PersistentProcess, PersistentReadiness, PersistentRun, PersistentStartErrorKind,
    PersistentStartup, ShutdownOutcome, persistent_start_error_kind, start_persistent_supervised,
    start_persistent_with_cancel,
};
pub use process_group::{
    interrupt as interrupt_process_group, isolate as isolate_process_group,
    isolate_async as isolate_process_group_async, kill as kill_process_group,
    terminate as terminate_process_group,
};
#[cfg(unix)]
pub use pty::{PtyIo, PtySize, terminal_size};
pub use result::ProcessResult;
pub use runner::{OutputObserver, run_with_cancel, run_with_cancel_supervised};
pub use supervisor::{
    ProcessSupervisor, RegistrationGuard, ShutdownSubscription, SupervisedAsyncChild,
    SupervisedBlockingChild,
};
pub use sync::{run, run_supervised};

/// Re-export error types
pub use rskit_errors::{AppError, AppResult, ErrorCode};

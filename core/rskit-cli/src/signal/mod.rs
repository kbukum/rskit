//! Caller-declared graceful shutdown over [`tokio_util::sync::CancellationToken`].
//!
//! rskit standardizes on `tokio_util`'s [`CancellationToken`] as its cooperative-cancellation type: long-running CLI work, worker handlers, and process supervision can all subscribe to the same token. [`ShutdownPolicy`] describes which operating-system signals begin shutdown, how long cooperative drain may run, and which non-zero code a second signal uses for immediate exit. [`ShutdownController`] installs that policy inside a Tokio runtime and owns the signal task for as long as the controller is retained.

mod controller;
mod policy;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use controller::{ShutdownController, on_ctrl_c};
pub use policy::{ShutdownPolicy, ShutdownSignal};
pub use tokio_util::sync::CancellationToken;

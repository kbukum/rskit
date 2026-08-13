//! Supervised subprocess lifetime ownership.

mod registry;
mod shutdown;
mod termination;
mod types;

pub use registry::RegistrationGuard;
pub use shutdown::ShutdownSubscription;
pub use types::{ProcessSupervisor, ShutdownReason, SupervisedAsyncChild, SupervisedBlockingChild};

pub(crate) use termination::{terminate_and_reap, terminate_and_wait_async};

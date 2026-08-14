//! Supervised subprocess lifetime ownership.

mod registry;
mod shutdown;
mod target;
mod termination;
mod types;

pub use registry::RegistrationGuard;
pub use shutdown::ShutdownSubscription;
pub use types::{ProcessSupervisor, SupervisedAsyncChild, SupervisedBlockingChild};

pub(crate) use target::OwnedChild;
pub(crate) use termination::{AsyncReap, SyncReap, terminate_and_reap, terminate_and_wait_async};

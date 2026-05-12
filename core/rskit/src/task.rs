//! Supervised async tasks — wraps `tokio::spawn` with panic catching and structured logging.

use std::future::Future;

/// A handle to a supervised background task.
pub struct SupervisedTask {
    handle: tokio::task::JoinHandle<()>,
    name: &'static str,
}

impl SupervisedTask {
    /// Abort the task.
    pub fn abort(&self) {
        self.handle.abort();
    }

    /// Await the task to completion.
    ///
    /// # Errors
    /// Returns `Err` if the task panicked or was cancelled.
    pub async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.handle.await
    }

    /// Returns the task name.
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// Spawn a supervised background task that catches panics and logs them.
///
/// Prefer this over bare `tokio::spawn` for long-running background work.
pub fn supervise<F>(name: &'static str, fut: F) -> SupervisedTask
where
    F: Future<Output = ()> + Send + 'static,
{
    use futures::FutureExt as _;
    let handle = tokio::spawn(async move {
        match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
            Ok(()) => {}
            Err(panic) => {
                tracing::error!(
                    task = name,
                    ?panic,
                    "supervised task panicked — restarting is the caller's responsibility"
                );
            }
        }
    });
    SupervisedTask { handle, name }
}

//! Cancellable owned tasks.
//!
//! [`SpawnedTask`] is the canonical owner for a background task that must be
//! cooperatively stopped: it bundles a `CancellationToken` with the
//! `JoinHandle`, so every task has explicit ownership, cancellation, and a
//! bounded, drain-then-abort shutdown. [`TaskGroup`] owns a set of such tasks
//! and shuts them all down together. This replaces the hand-rolled
//! `cancel + handle` pairs duplicated across consumers, watchers, and servers.

use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// A single background task with cooperative cancellation.
///
/// Created by [`SpawnedTask::spawn`], which hands the body a child
/// `CancellationToken` to `select!` on. Stop it with [`shutdown`](Self::shutdown)
/// (cancel → await up to a grace period → abort) so a wedged task can never
/// block shutdown indefinitely. Extra teardown (closing connections, flushing
/// buffers) stays with the caller.
#[derive(Debug)]
pub struct SpawnedTask {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

impl SpawnedTask {
    /// Spawn `body` with a fresh cancellation token passed to the closure.
    ///
    /// The body should `select!` on `token.cancelled()` to exit promptly.
    pub fn spawn<F, Fut>(body: F) -> Self
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(body(cancel.clone()));
        Self { cancel, handle }
    }

    /// Spawn `body` with the supplied cancellation token (e.g. a shared parent).
    pub fn spawn_with<Fut>(cancel: CancellationToken, body: Fut) -> Self
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(body);
        Self { cancel, handle }
    }

    /// Adopt an already-spawned `JoinHandle` paired with the token that
    /// stops it. For callers that spawn the task themselves but still want
    /// the bounded drain-then-abort shutdown.
    #[must_use]
    pub const fn from_parts(cancel: CancellationToken, handle: JoinHandle<()>) -> Self {
        Self { cancel, handle }
    }

    /// Signal the task to stop without waiting for it to finish.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Return `true` once the task has finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// A clone of the task's cancellation token, for cooperative shutdown.
    #[must_use]
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Cancel the task and wait up to `grace` for it to drain, aborting if it
    /// overruns. Always resolves; never blocks shutdown indefinitely.
    pub async fn shutdown(self, grace: Duration) {
        self.cancel.cancel();
        let mut handle = self.handle;
        if tokio::time::timeout(grace, &mut handle).await.is_err() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Abort the task immediately without waiting.
    pub fn abort(self) {
        self.handle.abort();
    }

    /// Wait for the task to finish without cancelling it.
    pub async fn join(self) {
        let _ = self.handle.await;
    }
}

/// A set of [`SpawnedTask`]s shut down together.
///
/// Each task owns its own cancellation token; [`shutdown`](Self::shutdown)
/// cancels all of them, then drains each within a per-task grace period,
/// aborting stragglers so a single wedged task cannot stall teardown.
#[derive(Debug, Default)]
pub struct TaskGroup {
    tasks: Vec<SpawnedTask>,
}

impl TaskGroup {
    /// Create an empty group.
    #[must_use]
    pub const fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Add a task to the group.
    pub fn push(&mut self, task: SpawnedTask) {
        self.tasks.push(task);
    }

    /// Number of tasks currently owned.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the group owns no tasks.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Cancel every task without waiting.
    pub fn cancel_all(&self) {
        for task in &self.tasks {
            task.cancel();
        }
    }

    /// Cancel and drain every task, each within `grace`, aborting stragglers.
    pub async fn shutdown(&mut self, grace: Duration) {
        let tasks = std::mem::take(&mut self.tasks);
        for task in &tasks {
            task.cancel();
        }
        for task in tasks {
            task.shutdown(grace).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn shutdown_cancels_cooperative_task() {
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();
        let task = SpawnedTask::spawn(move |cancel| async move {
            cancel.cancelled().await;
            r.store(true, Ordering::SeqCst);
        });
        task.shutdown(Duration::from_secs(1)).await;
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_aborts_wedged_task() {
        let task = SpawnedTask::spawn(|_cancel| async move {
            std::future::pending::<()>().await;
        });
        // Returns even though the body ignores cancellation.
        task.shutdown(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn group_shuts_down_all() {
        let mut group = TaskGroup::new();
        for _ in 0..3 {
            group.push(SpawnedTask::spawn(|cancel| async move {
                cancel.cancelled().await;
            }));
        }
        assert_eq!(group.len(), 3);
        group.shutdown(Duration::from_secs(1)).await;
        assert!(group.is_empty());
    }

    #[tokio::test]
    async fn task_constructors_cancel_abort_and_join_paths() {
        let token = CancellationToken::new();
        let finished = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&finished);
        let task = SpawnedTask::spawn_with(token.clone(), async move {
            token.cancelled().await;
            done.store(true, Ordering::SeqCst);
        });
        assert!(!task.is_finished());
        let cancellation = task.cancellation();
        task.cancel();
        task.join().await;
        assert!(cancellation.is_cancelled());
        assert!(finished.load(Ordering::SeqCst));

        let token = CancellationToken::new();
        let handle = tokio::spawn(async move {
            std::future::pending::<()>().await;
        });
        let task = SpawnedTask::from_parts(token, handle);
        task.abort();
    }

    #[tokio::test]
    async fn cancel_all_cancels_without_waiting() {
        let mut group = TaskGroup::new();
        let token = CancellationToken::new();
        let task = SpawnedTask::from_parts(
            token.clone(),
            tokio::spawn(async move {
                std::future::pending::<()>().await;
            }),
        );
        group.push(task);

        group.cancel_all();

        assert!(token.is_cancelled());
        group.shutdown(Duration::from_millis(1)).await;
    }
}

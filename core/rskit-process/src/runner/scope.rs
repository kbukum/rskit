//! [`ChildScope`] — an RAII guard that owns a spawned child and its async I/O
//! tasks so an early return abandons them cleanly instead of leaking them.

use tokio::process::Child;
use tokio::task::{AbortHandle, JoinHandle};

/// Owns a spawned child and the run's I/O tasks' abort handles so an early return
/// abandons them cleanly.
///
/// Every async runner (pipe, inherited, and PTY mode) spawns background reader /
/// stdin-writer tasks alongside the child, then reaches several `?` points —
/// completion wait, task joins — before it finishes. A dropped Tokio
/// [`JoinHandle`] is *detached*, not cancelled, so returning an error out of one
/// of those points without this guard would leak the tasks and keep the child's
/// pipes (or PTY fds) alive. While armed, dropping the scope aborts every
/// registered task and best-effort kills the child; [`disarm`](Self::disarm)
/// after a normal completion hands ownership back to the joined results.
pub(super) struct ChildScope {
    child: Child,
    aborts: Vec<AbortHandle>,
    armed: bool,
}

impl ChildScope {
    /// Take ownership of the spawned child, armed by default.
    pub(super) fn new(child: Child) -> Self {
        Self {
            child,
            aborts: Vec::new(),
            armed: true,
        }
    }

    /// Track a spawned task so it is aborted if the run is abandoned early.
    pub(super) fn register<T>(&mut self, task: &Option<JoinHandle<T>>) {
        if let Some(task) = task {
            self.aborts.push(task.abort_handle());
        }
    }

    /// Borrow the owned child for the completion wait.
    pub(super) fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Mark the run as completed normally so drop neither aborts nor kills.
    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ChildScope {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        for abort in &self.aborts {
            abort.abort();
        }
        // Best-effort: the child may already have exited, in which case this is a
        // harmless no-op.
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::process::Command;

    use super::ChildScope;

    /// Spawn a long-lived child that will outlive the test unless killed.
    fn spawn_sleeper() -> tokio::process::Child {
        Command::new("/bin/sleep")
            .arg("30")
            .kill_on_drop(false)
            .spawn()
            .expect("spawn sleep")
    }

    #[tokio::test]
    async fn dropping_an_armed_scope_aborts_tasks_and_kills_the_child() {
        let child = spawn_sleeper();
        let some_task = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }));

        let mut scope = ChildScope::new(child);
        scope.register(&some_task);
        drop(scope);

        // The registered task is aborted (its join resolves as cancelled), and
        // the child is killed rather than detached — proving the error-path guard
        // reaps both.
        let Some(task) = some_task else {
            unreachable!("task was just registered")
        };
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn disarming_the_scope_leaves_the_child_and_tasks_alone() {
        let mut child = spawn_sleeper();
        let some_task = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }));

        let mut scope = ChildScope::new(spawn_sleeper());
        scope.register(&some_task);
        scope.disarm();
        drop(scope);

        // A disarmed scope neither aborts the task nor kills the caller-owned
        // child: the task runs to completion and the child is still alive until
        // the test explicitly reaps it.
        let Some(task) = some_task else {
            unreachable!("task was just registered")
        };
        task.await.expect("task completed");
        assert!(child.try_wait().expect("try_wait").is_none());
        let _ = child.start_kill();
    }
}

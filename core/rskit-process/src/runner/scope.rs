//! [`ChildScope`] — an RAII guard that owns a spawned child and its async I/O tasks
//! so an early return abandons them cleanly instead of leaking them.

use tokio::process::Child;
use tokio::runtime::Handle;
use tokio::task::{AbortHandle, JoinHandle};

use crate::{
    LifecyclePolicy, RegistrationGuard, process_group::kill_target, supervisor::OwnedChild,
};
/// Owns a spawned child and the run's I/O tasks' abort handles
/// so an early return abandons them cleanly.
///
/// Every async runner (pipe, inherited, and PTY mode) spawns background reader / stdin-writer tasks alongside the child,
/// then reaches several `?` points — completion wait, task joins — before it finishes.
/// A dropped Tokio [`JoinHandle`] is *detached*, not cancelled,
/// so returning an error out of one of those points without this guard would leak the tasks
/// and keep the child's pipes (or PTY fds) alive. While armed,
/// dropping the scope aborts every registered task and kills **and reaps** the child;
/// [`disarm`](Self::disarm) after a normal completion hands ownership back to the joined results.
pub(super) struct ChildScope {
    child: Option<Child>,
    registration: Option<RegistrationGuard>,
    aborts: Vec<AbortHandle>,
    lifecycle: LifecyclePolicy,
    armed: bool,
}

impl ChildScope {
    /// Take ownership of the spawned child, armed by default.
    pub(super) fn new(
        child: Child,
        registration: RegistrationGuard,
        lifecycle: LifecyclePolicy,
    ) -> Self {
        Self {
            child: Some(child),
            registration: Some(registration),
            aborts: Vec::new(),
            lifecycle,
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
    ///
    /// The child is present for the scope's whole life; it is taken only while dropping the scope,
    /// after which no method is invoked.
    pub(super) fn child_mut(&mut self) -> &mut Child {
        match &mut self.child {
            Some(child) => child,
            // The child is removed only in `Drop`, and no method runs after the scope is dropped.
            // A `&mut Child` cannot be conjured,
            // so there is no graceful value to return for this structurally-unreachable arm.
            None => unreachable!("ChildScope::child_mut called after the child was taken"),
        }
    }

    /// Mark the run as completed normally so drop neither aborts nor kills.
    pub(super) fn disarm(&mut self) {
        self.armed = false;
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
    }

    /// Relinquish a still-live child (it survived its grace period with
    /// escalation disabled) to its owned target without killing or unregistering.
    ///
    /// The child moves into the registered target so a later shutdown or
    /// supervisor drop reaps it; the guard is retained so scope drop neither kills
    /// the child nor removes the entry.
    pub(super) fn relinquish(&mut self) {
        self.armed = false;
        if let (Some(registration), Some(child)) = (self.registration.take(), self.child.take()) {
            registration.relinquish_child(OwnedChild::Tokio(child));
        }
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
        // Kill *and reap* the child so an early error after spawn —
        // e.g. an I/O task setup failure (PTY fd clone / `AsyncFd` registration) before the completion wait ever ran
        // — cannot leave a zombie on Unix. `Drop` is synchronous,
        // so the async reap is handed to a detached task on the current runtime;
        // `start_kill` already fired, so even without a running runtime the child is killed
        // and Tokio's orphan reaper collects it.
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = child.id() {
                let _ = kill_target(pid, self.lifecycle.targets_group());
            }
            let _ = child.start_kill();
            if let Ok(handle) = Handle::try_current() {
                handle.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::process::Command;

    use super::ChildScope;
    use crate::{LifecyclePolicy, ProcessSupervisor};

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

        let supervisor = ProcessSupervisor::new(LifecyclePolicy::default());
        let registration = supervisor.track_pid(child.id().unwrap_or_default());
        let mut scope = ChildScope::new(child, registration, LifecyclePolicy::default());
        scope.register(&some_task);
        drop(scope);

        // The registered task is aborted (its join resolves as cancelled), and the child is killed
        // and reaped by the detached reaper task the guard spawns on drop —
        // proving the error path abandons neither.
        let Some(task) = some_task else {
            unreachable!("task was just registered")
        };
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn disarming_the_scope_leaves_the_child_and_tasks_alone() {
        let some_task = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }));

        let supervisor = ProcessSupervisor::new(LifecyclePolicy::default());
        let child = spawn_sleeper();
        let registration = supervisor.track_pid(child.id().unwrap_or_default());
        let mut scope = ChildScope::new(child, registration, LifecyclePolicy::default());
        scope.register(&some_task);
        scope.disarm();

        // A disarmed scope neither aborts the task nor kills the child: the task runs to completion
        // and the child is still alive after disarming.
        let Some(task) = some_task else {
            unreachable!("task was just registered")
        };
        task.await.expect("task completed");
        assert!(
            scope.child_mut().try_wait().expect("try_wait").is_none(),
            "a disarmed scope must not kill the child"
        );

        // Explicitly reap the child through the still-owning scope
        // so the test leaves no long-lived subprocess behind.
        scope.child_mut().start_kill().expect("kill child");
        scope.child_mut().wait().await.expect("reap child");
    }
}

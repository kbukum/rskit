//! Caller-facing subprocess lifecycle policy.

use std::time::Duration;

/// Policy for spawned-child lifetime, isolation, and shutdown escalation.
///
/// The policy is consumed by sync, async, persistent, and supervised spawns. When isolation is enabled the child is placed in its own process group so termination can target the whole group. Cleanup relies on Rust-side drops and explicit supervision: on a hard parent death (which runs no destructors) no platform can guarantee the group is reaped, so long-lived children should be supervised explicitly. Children are otherwise reaped on normal shutdown, timeout, cancellation, future drop, and supervisor drop.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct LifecyclePolicy {
    /// Grace period to wait after graceful termination before kill escalation.
    pub grace_period: Duration,
    /// Create a new process group/session where supported.
    pub isolate_process_group: bool,
    /// Terminate the process group rather than only the immediate child where supported.
    pub terminate_descendants: bool,
    /// Escalate to a force kill after the grace period expires.
    pub kill_after_grace: bool,
}

impl Default for LifecyclePolicy {
    fn default() -> Self {
        Self {
            grace_period: Duration::from_secs(5),
            isolate_process_group: true,
            terminate_descendants: true,
            kill_after_grace: true,
        }
    }
}

impl LifecyclePolicy {
    /// Set the graceful termination period before kill escalation.
    #[must_use]
    pub fn with_grace_period(mut self, grace_period: Duration) -> Self {
        self.grace_period = grace_period;
        self
    }

    /// Set whether processes are spawned into a new process group where supported.
    #[must_use]
    pub fn with_isolate_process_group(mut self, isolate_process_group: bool) -> Self {
        self.isolate_process_group = isolate_process_group;
        self
    }

    /// Set whether termination targets descendants through the process group.
    #[must_use]
    pub fn with_terminate_descendants(mut self, terminate_descendants: bool) -> Self {
        self.terminate_descendants = terminate_descendants;
        self
    }

    /// Set whether shutdown escalates to a force kill after the grace period expires.
    ///
    /// Setting this to `false` requests "leave a still-running process alone rather
    /// than `SIGKILL` it after the grace period". This is only honored end-to-end
    /// under an **injected, long-lived supervisor**: the survivor is relinquished to
    /// that supervisor and reaped on its shutdown or drop, so it is left running for
    /// the supervisor's lifetime instead of being force-killed — leak-safe, never
    /// detached forever.
    ///
    /// The call-scoped APIs [`crate::run`] and [`crate::run_with_cancel`] create a
    /// per-call supervisor that owns the child for the duration of the call and
    /// reaps any still-registered survivor when it drops as the call returns. With
    /// `kill_after_grace = false` such a survivor is therefore reaped on return
    /// rather than escalated mid-call — the default supervisor "just works" and does
    /// not leak, but a call-scoped spawn cannot outlive the call that made it.
    ///
    /// A persistent spawn without an injected supervisor ([`start_persistent`])
    /// likewise creates its own supervisor, but the returned handle owns it for the
    /// handle's lifetime, so a `kill_after_grace = false` survivor is reaped when
    /// the handle is dropped rather than at any single call boundary.
    ///
    /// To keep a process alive independently of a single call or handle, inject a
    /// long-lived supervisor that owns it.
    ///
    /// [`start_persistent`]: crate::start_persistent_with_cancel
    #[must_use]
    pub fn with_kill_after_grace(mut self, kill_after_grace: bool) -> Self {
        self.kill_after_grace = kill_after_grace;
        self
    }

    pub(crate) const fn targets_group(self) -> bool {
        self.isolate_process_group && self.terminate_descendants
    }
}

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
    #[must_use]
    pub fn with_kill_after_grace(mut self, kill_after_grace: bool) -> Self {
        self.kill_after_grace = kill_after_grace;
        self
    }

    pub(crate) const fn targets_group(self) -> bool {
        self.isolate_process_group && self.terminate_descendants
    }
}

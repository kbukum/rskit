//! Signal and process-tree termination policy.

use std::time::Duration;

/// Signal and process-tree termination policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct SignalPolicy {
    /// Grace period to wait after graceful termination before killing.
    pub grace_period: Duration,
    /// Create a new process group/session where supported.
    pub create_process_group: bool,
    /// Terminate the process group rather than only the immediate child where supported.
    pub terminate_descendants: bool,
}

impl Default for SignalPolicy {
    fn default() -> Self {
        Self {
            grace_period: Duration::from_secs(5),
            create_process_group: true,
            terminate_descendants: true,
        }
    }
}

impl SignalPolicy {
    /// Set the graceful termination period before kill escalation.
    #[must_use]
    pub fn with_grace_period(mut self, grace_period: Duration) -> Self {
        self.grace_period = grace_period;
        self
    }

    /// Set whether processes are spawned into a new process group where supported.
    #[must_use]
    pub fn with_create_process_group(mut self, create_process_group: bool) -> Self {
        self.create_process_group = create_process_group;
        self
    }

    /// Set whether termination targets descendants through the process group.
    #[must_use]
    pub fn with_terminate_descendants(mut self, terminate_descendants: bool) -> Self {
        self.terminate_descendants = terminate_descendants;
        self
    }
}

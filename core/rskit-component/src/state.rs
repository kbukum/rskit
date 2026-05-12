use std::fmt;

/// Lifecycle state tracked for each registered component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum State {
    /// The component has been registered but not started.
    Created,
    /// The component is currently starting.
    Starting,
    /// The component is running.
    Running,
    /// The component is currently stopping.
    Stopping,
    /// The component has stopped cleanly.
    Stopped,
    /// The component failed to start or stop cleanly.
    Failed,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => f.write_str("created"),
            Self::Starting => f.write_str("starting"),
            Self::Running => f.write_str("running"),
            Self::Stopping => f.write_str("stopping"),
            Self::Stopped => f.write_str("stopped"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

impl State {
    pub(crate) const fn can_start(self) -> bool {
        matches!(self, Self::Created | Self::Stopped | Self::Failed)
    }

    pub(crate) const fn should_stop(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn display_matches_expected_values() {
        assert_eq!(State::Created.to_string(), "created");
        assert_eq!(State::Starting.to_string(), "starting");
        assert_eq!(State::Running.to_string(), "running");
        assert_eq!(State::Stopping.to_string(), "stopping");
        assert_eq!(State::Stopped.to_string(), "stopped");
        assert_eq!(State::Failed.to_string(), "failed");
    }
}

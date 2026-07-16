//! Workload runtime state and restart policy.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a workload as reported by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkloadState {
    /// Created but not yet started.
    Created,
    /// Running.
    Running,
    /// Stopped by request.
    Stopped,
    /// Ran to completion successfully.
    Completed,
    /// Terminated with an error.
    Error,
    /// Restarting.
    Restarting,
    /// State could not be determined.
    #[default]
    Unknown,
    /// No such workload exists.
    NotFound,
}

impl WorkloadState {
    /// Returns `true` when the workload is actively running.
    #[must_use]
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    /// Returns `true` when the workload has reached a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Completed | Self::Error | Self::NotFound
        )
    }

    /// Machine-readable identifier for the state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Completed => "completed",
            Self::Error => "error",
            Self::Restarting => "restarting",
            Self::Unknown => "unknown",
            Self::NotFound => "not_found",
        }
    }
}

impl std::fmt::Display for WorkloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Policy governing whether a workload is restarted after it exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RestartPolicy {
    /// Never restart.
    #[default]
    No,
    /// Always restart.
    Always,
    /// Restart only on non-zero exit.
    OnFailure,
    /// Restart unless explicitly stopped.
    UnlessStopped,
}

impl RestartPolicy {
    /// Machine-readable identifier for the policy.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Always => "always",
            Self::OnFailure => "on-failure",
            Self::UnlessStopped => "unless-stopped",
        }
    }
}

impl std::fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_default_is_unknown() {
        assert_eq!(WorkloadState::default(), WorkloadState::Unknown);
    }

    #[test]
    fn state_running_and_terminal_classification() {
        assert!(WorkloadState::Running.is_running());
        assert!(!WorkloadState::Running.is_terminal());
        assert!(WorkloadState::Completed.is_terminal());
        assert!(WorkloadState::Error.is_terminal());
        assert!(!WorkloadState::Created.is_terminal());
    }

    #[test]
    fn state_as_str_covers_every_variant() {
        assert_eq!(WorkloadState::Created.as_str(), "created");
        assert_eq!(WorkloadState::Running.as_str(), "running");
        assert_eq!(WorkloadState::Stopped.as_str(), "stopped");
        assert_eq!(WorkloadState::Completed.as_str(), "completed");
        assert_eq!(WorkloadState::Error.as_str(), "error");
        assert_eq!(WorkloadState::Restarting.as_str(), "restarting");
        assert_eq!(WorkloadState::Unknown.as_str(), "unknown");
        assert_eq!(WorkloadState::NotFound.as_str(), "not_found");
    }

    #[test]
    fn state_display_matches_as_str() {
        assert_eq!(WorkloadState::NotFound.to_string(), "not_found");
        assert_eq!(WorkloadState::Restarting.as_str(), "restarting");
    }

    #[test]
    fn state_serializes_snake_case() {
        let json = serde_json::to_string(&WorkloadState::NotFound).unwrap();
        assert_eq!(json, "\"not_found\"");
    }

    #[test]
    fn restart_policy_default_is_no() {
        assert_eq!(RestartPolicy::default(), RestartPolicy::No);
    }

    #[test]
    fn restart_policy_as_str_covers_every_variant() {
        assert_eq!(RestartPolicy::No.as_str(), "no");
        assert_eq!(RestartPolicy::Always.as_str(), "always");
        assert_eq!(RestartPolicy::OnFailure.as_str(), "on-failure");
        assert_eq!(RestartPolicy::UnlessStopped.as_str(), "unless-stopped");
    }

    #[test]
    fn restart_policy_display_uses_kebab_case() {
        assert_eq!(RestartPolicy::OnFailure.to_string(), "on-failure");
        assert_eq!(RestartPolicy::UnlessStopped.as_str(), "unless-stopped");
    }

    #[test]
    fn restart_policy_serializes_kebab_case() {
        let json = serde_json::to_string(&RestartPolicy::OnFailure).unwrap();
        assert_eq!(json, "\"on-failure\"");
    }
}

use crate::{AppError, ErrorCode};

const PERSISTENT_START_ERROR_KIND_DETAIL: &str = "rskit_process.persistent_start_error_kind";

/// Machine-readable persistent process startup failure classification.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistentStartErrorKind {
    /// The persistent process could not be spawned.
    SpawnFailed,
    /// The readiness command timed out.
    ReadinessCommandTimedOut,
    /// The readiness command exited unsuccessfully.
    ReadinessCommandFailed,
    /// The process did not become ready before the readiness timeout.
    ReadinessTimedOut,
    /// Output streams ended before output readiness was observed.
    OutputEndedBeforeReadiness,
    /// The process exited before becoming ready.
    ExitedBeforeReadiness,
}

impl PersistentStartErrorKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SpawnFailed => "spawn_failed",
            Self::ReadinessCommandTimedOut => "readiness_command_timed_out",
            Self::ReadinessCommandFailed => "readiness_command_failed",
            Self::ReadinessTimedOut => "readiness_timed_out",
            Self::OutputEndedBeforeReadiness => "output_ended_before_readiness",
            Self::ExitedBeforeReadiness => "exited_before_readiness",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "spawn_failed" => Some(Self::SpawnFailed),
            "readiness_command_timed_out" => Some(Self::ReadinessCommandTimedOut),
            "readiness_command_failed" => Some(Self::ReadinessCommandFailed),
            "readiness_timed_out" => Some(Self::ReadinessTimedOut),
            "output_ended_before_readiness" => Some(Self::OutputEndedBeforeReadiness),
            "exited_before_readiness" => Some(Self::ExitedBeforeReadiness),
            _ => None,
        }
    }
}

/// Return the structured persistent startup error kind attached to an [`AppError`].
pub fn persistent_start_error_kind(error: &AppError) -> Option<PersistentStartErrorKind> {
    error
        .details
        .get(PERSISTENT_START_ERROR_KIND_DETAIL)
        .and_then(|value| value.as_str())
        .and_then(PersistentStartErrorKind::from_str)
}

pub(in crate::persistent) fn persistent_start_error(
    kind: PersistentStartErrorKind,
    code: ErrorCode,
    message: impl Into<String>,
) -> AppError {
    AppError::new(code, message).with_detail(PERSISTENT_START_ERROR_KIND_DETAIL, kind.as_str())
}

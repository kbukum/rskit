use rskit_ai::FinishReason;
use serde::{Deserialize, Serialize};

/// Why the agent loop terminated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StopReason {
    /// The model finished its response without requesting any tool calls.
    EndTurn,
    /// Reached the configured maximum number of turns.
    MaxTurns,
    /// Exceeded the token budget.
    MaxTokens,
    /// Exceeded the wall-clock budget.
    WallClockExceeded,
    /// Exceeded the maximum tool-call budget.
    MaxToolCallsExceeded,
    /// The run was cancelled.
    Cancelled,
    /// Aborted due to a hook handler, model error, or content filter.
    Aborted,
}

impl From<FinishReason> for StopReason {
    fn from(reason: FinishReason) -> Self {
        match reason {
            FinishReason::Length => Self::MaxTokens,
            FinishReason::Cancelled => Self::Cancelled,
            FinishReason::Error | FinishReason::ContentFilter => Self::Aborted,
            _ => Self::EndTurn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_reason_serde() {
        let json = serde_json::to_string(&StopReason::EndTurn).unwrap();
        let deser: StopReason = serde_json::from_str(&json).unwrap();
        assert!(matches!(deser, StopReason::EndTurn));
    }
}

use rskit_ai::FinishReason;
use serde::{Deserialize, Serialize};

/// Why the agent loop terminated.
///
/// Serializes with gokit's stop-reason wire strings for cross-kit parity, while the
/// variant set and rskit's limit-error precedence (which limit wins when several apply)
/// are resolved in the crate's `runtime::stop` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StopReason {
    /// The model finished its response without requesting any tool calls.
    #[serde(rename = "stop")]
    EndTurn,
    /// Reached the configured maximum number of turns.
    #[serde(rename = "max_turns")]
    MaxTurns,
    /// Exceeded the token budget.
    #[serde(rename = "length")]
    MaxTokens,
    /// Exceeded the wall-clock budget.
    #[serde(rename = "wall_clock")]
    WallClockExceeded,
    /// Exceeded the maximum tool-call budget.
    #[serde(rename = "max_tool_calls")]
    MaxToolCallsExceeded,
    /// The run was cancelled.
    #[serde(rename = "cancelled")]
    Cancelled,
    /// Aborted due to a hook handler, model error, or content filter.
    #[serde(rename = "error")]
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

    #[test]
    fn stop_reason_uses_gokit_wire_strings() {
        let cases = [
            (StopReason::EndTurn, "stop"),
            (StopReason::MaxTurns, "max_turns"),
            (StopReason::MaxTokens, "length"),
            (StopReason::WallClockExceeded, "wall_clock"),
            (StopReason::MaxToolCallsExceeded, "max_tool_calls"),
            (StopReason::Cancelled, "cancelled"),
            (StopReason::Aborted, "error"),
        ];
        for (reason, wire) in cases {
            assert_eq!(
                serde_json::to_value(&reason).unwrap(),
                serde_json::json!(wire)
            );
        }
    }
}

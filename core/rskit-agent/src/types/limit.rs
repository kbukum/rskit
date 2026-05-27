use serde::{Deserialize, Serialize};

/// Agent limit or cancellation failure with locked precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AgentLimitError {
    /// Parent cancellation wins over all budget failures.
    Cancelled,
    /// Wall-clock deadline exceeded.
    WallClockExceeded,
    /// Maximum tool-call budget exceeded.
    MaxToolCallsExceeded,
    /// Maximum token budget exceeded.
    MaxTokensExceeded,
    /// Maximum turn budget exceeded.
    MaxTurnsExceeded,
}

impl AgentLimitError {
    /// Return precedence where larger values win.
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Cancelled => 5,
            Self::WallClockExceeded => 4,
            Self::MaxToolCallsExceeded => 3,
            Self::MaxTokensExceeded => 2,
            Self::MaxTurnsExceeded => 1,
        }
    }
}

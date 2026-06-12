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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_outranks_every_budget_failure() {
        let budgets = [
            AgentLimitError::WallClockExceeded,
            AgentLimitError::MaxToolCallsExceeded,
            AgentLimitError::MaxTokensExceeded,
            AgentLimitError::MaxTurnsExceeded,
        ];
        for budget in budgets {
            assert!(
                AgentLimitError::Cancelled.precedence() > budget.precedence(),
                "cancellation must win over {budget:?}",
            );
        }
    }

    #[test]
    fn precedence_is_strict_total_order_with_locked_ranking() {
        // Highest-to-lowest priority is a stable, documented contract.
        let ranked = [
            AgentLimitError::Cancelled,
            AgentLimitError::WallClockExceeded,
            AgentLimitError::MaxToolCallsExceeded,
            AgentLimitError::MaxTokensExceeded,
            AgentLimitError::MaxTurnsExceeded,
        ];
        for pair in ranked.windows(2) {
            assert!(
                pair[0].precedence() > pair[1].precedence(),
                "{:?} must outrank {:?}",
                pair[0],
                pair[1],
            );
        }

        // Selecting the winner by max precedence is order-independent.
        let mut shuffled = ranked;
        shuffled.reverse();
        let winner = shuffled
            .into_iter()
            .max_by_key(|error| error.precedence())
            .expect("non-empty");
        assert_eq!(winner, AgentLimitError::Cancelled);
    }
}

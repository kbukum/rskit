//! Budget vocabulary shared by GenAI orchestration layers.

use serde::{Deserialize, Serialize};

use crate::usage::Cost;

/// Budget vocabulary shared by agent/tool/LLM orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum total tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum model/tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<u64>,
    /// Maximum cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<Cost>,
    /// Wall-clock budget in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock: Option<u64>,
}

/// Reason a budget was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BudgetExceededReason {
    /// Token budget exceeded.
    Tokens,
    /// Call budget exceeded.
    Calls,
    /// Cost budget exceeded.
    Cost,
    /// Wall-clock budget exceeded.
    WallClock,
    /// Operation cancelled before completion.
    Cancelled,
}

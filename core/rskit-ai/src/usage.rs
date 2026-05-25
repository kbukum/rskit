//! Usage and cost vocabulary.

use serde::{Deserialize, Serialize};

/// Token usage counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt/input tokens consumed.
    pub input_tokens: u64,
    /// Completion/output tokens produced.
    pub output_tokens: u64,
    /// Tokens served from provider cache.
    #[serde(default)]
    pub cached_tokens: u64,
    /// Reasoning tokens reported by providers that expose them.
    #[serde(default)]
    pub reasoning_tokens: u64,
}

/// Decimal money amount represented in millionths of the currency unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    /// Amount in millionths of the currency unit.
    pub micros: i128,
}

/// Cost breakdown for an AI operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    /// Input token cost.
    pub input: Money,
    /// Output token cost.
    pub output: Money,
    /// Cached token cost.
    pub cached: Money,
    /// Reasoning token cost.
    pub reasoning: Money,
    /// ISO 4217 currency code.
    pub currency: String,
}

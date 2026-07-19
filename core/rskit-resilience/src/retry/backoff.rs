//! Backoff strategies and the backoff algorithm selector.

use std::time::Duration;

/// Fixed retry backoff that uses the same delay for every retry attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantBackoff {
    /// Delay applied to every retry attempt.
    pub delay: Duration,
}

impl ConstantBackoff {
    /// Create a new constant backoff strategy.
    #[must_use]
    pub fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

/// Linear retry backoff that increases by a constant increment each attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearBackoff {
    /// Delay used for the first retry.
    pub initial_backoff: Duration,
    /// Increment added for each subsequent retry.
    pub increment: Duration,
    /// Upper bound applied to the computed delay.
    pub max_backoff: Duration,
}

impl LinearBackoff {
    /// Create a new linear backoff strategy.
    #[must_use]
    pub fn new(initial_backoff: Duration, increment: Duration, max_backoff: Duration) -> Self {
        Self {
            initial_backoff,
            increment,
            max_backoff,
        }
    }
}

/// Backoff algorithm used by a [`RetryPolicy`](super::RetryPolicy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackoffKind {
    /// Exponential backoff using `initial_backoff * backoff_factor^(attempt-1)`.
    Exponential,
    /// Fixed delay for every retry attempt.
    Constant,
    /// Linearly increasing delay using `initial_backoff + increment * (attempt-1)`.
    Linear,
}

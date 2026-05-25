//! Typed GenAI error sentinels.

use thiserror::Error;

use crate::BudgetExceededReason;

/// Typed AI error sentinels.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum GenAiError {
    /// Provider rate limit was hit.
    #[error("rate limited")]
    RateLimited,
    /// Request exceeded model context length.
    #[error("context length exceeded")]
    ContextLengthExceeded,
    /// Content filter rejected the request or response.
    #[error("content filtered")]
    ContentFilter,
    /// Model is overloaded.
    #[error("model overloaded")]
    ModelOverloaded,
    /// Budget was exceeded.
    #[error("budget exceeded: {0:?}")]
    BudgetExceeded(BudgetExceededReason),
    /// Requested model was not found.
    #[error("model not found")]
    ModelNotFound,
    /// Request was invalid.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

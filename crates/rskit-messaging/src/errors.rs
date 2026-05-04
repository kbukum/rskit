//! Error classification for retry and circuit-breaker decisions.

use rskit_errors::AppError;

/// Classifies errors for retry and circuit-breaker decisions.
///
/// Broker adapters implement this trait so that generic retry policies and
/// circuit breakers can decide how to handle a particular failure without
/// knowing which broker produced it.
pub trait ErrorClassifier: Send + Sync {
    /// Returns `true` when the error indicates the broker connection is down
    /// (e.g. TCP reset, DNS failure, authentication timeout).
    fn is_connection_error(&self, err: &AppError) -> bool;

    /// Returns `true` when the error is transient and the operation can be
    /// retried (e.g. leader election in progress, throttling, temporary I/O
    /// error).  Connection errors are typically retryable too, but the
    /// distinction lets callers apply different back-off strategies.
    fn is_retryable_error(&self, err: &AppError) -> bool;
}

/// A no-op classifier that considers every error non-retryable.
///
/// Useful as a default when no broker-specific classifier is available.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopErrorClassifier;

impl ErrorClassifier for NoopErrorClassifier {
    fn is_connection_error(&self, _err: &AppError) -> bool {
        false
    }

    fn is_retryable_error(&self, _err: &AppError) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::{AppError, ErrorCode};

    use super::*;

    #[test]
    fn noop_classifier_returns_false() {
        let classifier = NoopErrorClassifier;
        let err = AppError::new(ErrorCode::Internal, "something broke");

        assert!(!classifier.is_connection_error(&err));
        assert!(!classifier.is_retryable_error(&err));
    }
}

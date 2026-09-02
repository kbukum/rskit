//! Typed transport-level error classification for the HTTP client.
//!
//! Transport failures surface as [`rskit_errors::AppError`] so callers keep the shared error
//! type, cause chain, and native retryable metadata. [`TransportErrorKind`] adds a stable,
//! cross-kit classification over the three transport-level failure categories — [`Timeout`],
//! [`Connection`], and [`ResponseTooLarge`] — recoverable from an error via
//! [`TransportErrorKind::classify`].
//!
//! [`Timeout`]: TransportErrorKind::Timeout
//! [`Connection`]: TransportErrorKind::Connection
//! [`ResponseTooLarge`]: TransportErrorKind::ResponseTooLarge

use rskit_errors::{AppError, ErrorCode};

/// Detail key under which a transport error records its [`TransportErrorKind`].
pub(crate) const TRANSPORT_KIND_DETAIL: &str = "transport_kind";

/// Transport-level classification of an HTTP client failure.
///
/// A [`Timeout`](Self::Timeout) or [`Connection`](Self::Connection) failure is transient and
/// retryable; a [`ResponseTooLarge`](Self::ResponseTooLarge) failure is not, since retrying only
/// reproduces the oversized response. The retryable decision is carried natively by
/// [`AppError::is_retryable`], so resilience policies retry these errors without inspecting the
/// kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportErrorKind {
    /// A request or connection deadline was exceeded.
    Timeout,
    /// A connection could not be established (refused, DNS failure, reset).
    Connection,
    /// The response body exceeded the configured maximum size.
    ResponseTooLarge,
}

impl TransportErrorKind {
    /// Stable lowercase label shared across kits.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connection => "connection",
            Self::ResponseTooLarge => "response_too_large",
        }
    }

    /// Whether an operation failing with this kind is safe to retry.
    #[must_use]
    pub fn retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Connection)
    }

    /// Canonical [`ErrorCode`] carried by an error of this kind.
    pub(crate) fn error_code(self) -> ErrorCode {
        match self {
            Self::Timeout => ErrorCode::Timeout,
            Self::Connection => ErrorCode::ConnectionFailed,
            Self::ResponseTooLarge => ErrorCode::InvalidInput,
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "timeout" => Some(Self::Timeout),
            "connection" => Some(Self::Connection),
            "response_too_large" => Some(Self::ResponseTooLarge),
            _ => None,
        }
    }

    /// Recover the transport classification from an [`AppError`], if it carries one.
    #[must_use]
    pub fn classify(error: &AppError) -> Option<Self> {
        error
            .details()
            .get(TRANSPORT_KIND_DETAIL)
            .and_then(serde_json::Value::as_str)
            .and_then(Self::from_label)
    }
}

/// Build a transport [`AppError`] tagged with `kind`, deriving retryable from the kind.
pub(crate) fn transport_error(kind: TransportErrorKind, message: impl Into<String>) -> AppError {
    AppError::new(kind.error_code(), message)
        .retryable(kind.retryable())
        .with_detail(TRANSPORT_KIND_DETAIL, kind.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_only_for_transient_kinds() {
        assert!(TransportErrorKind::Timeout.retryable());
        assert!(TransportErrorKind::Connection.retryable());
        assert!(!TransportErrorKind::ResponseTooLarge.retryable());
    }

    #[test]
    fn transport_error_tags_kind_and_native_retryable() {
        let timeout = transport_error(TransportErrorKind::Timeout, "deadline exceeded");
        assert_eq!(timeout.code(), ErrorCode::Timeout);
        assert!(timeout.is_retryable());
        assert_eq!(
            TransportErrorKind::classify(&timeout),
            Some(TransportErrorKind::Timeout)
        );

        let too_large = transport_error(TransportErrorKind::ResponseTooLarge, "body too large");
        assert_eq!(too_large.code(), ErrorCode::InvalidInput);
        assert!(!too_large.is_retryable());
        assert_eq!(
            TransportErrorKind::classify(&too_large),
            Some(TransportErrorKind::ResponseTooLarge)
        );
    }

    #[test]
    fn classify_returns_none_for_unclassified_errors() {
        let generic = AppError::new(ErrorCode::ExternalService, "boom");
        assert_eq!(TransportErrorKind::classify(&generic), None);
    }
}

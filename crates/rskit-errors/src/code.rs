/// Machine-readable error code.
///
/// Defined as an enum so downstream code gets exhaustive match checking.
/// `#[non_exhaustive]` allows new variants to be added without a SemVer break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ErrorCode {
    // ── Connection / availability (all retryable) ──────────────────────
    /// The downstream service is unavailable; retryable.
    ServiceUnavailable,
    /// Could not establish a connection to a dependency; retryable.
    ConnectionFailed,
    /// An operation exceeded its deadline; retryable.
    Timeout,
    /// Request was rejected because a rate limit was exceeded; retryable.
    RateLimited,

    // ── Resource ───────────────────────────────────────────────────────
    /// The requested resource does not exist.
    NotFound,
    /// A resource with the same identity already exists.
    AlreadyExists,
    /// The request conflicts with the current state of a resource.
    Conflict,

    // ── Validation ─────────────────────────────────────────────────────
    /// One or more input fields failed validation.
    InvalidInput,
    /// A required field was absent from the request.
    MissingField,
    /// A field value could not be parsed into the expected format.
    InvalidFormat,

    // ── Auth ───────────────────────────────────────────────────────────
    /// The caller has not been authenticated.
    Unauthorized,
    /// The caller is authenticated but does not have permission.
    Forbidden,
    /// The authentication token has expired.
    TokenExpired,
    /// The authentication token is malformed or unrecognised.
    InvalidToken,

    // ── Internal ───────────────────────────────────────────────────────
    /// An unexpected internal error occurred.
    Internal,
    /// A database operation failed.
    DatabaseError,
    /// An external service returned an error; retryable.
    ExternalService,
    /// The operation was cancelled before completion.
    Cancelled,
}

impl ErrorCode {
    /// Returns `true` for transient errors worth retrying.
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::ServiceUnavailable
                | ErrorCode::ConnectionFailed
                | ErrorCode::Timeout
                | ErrorCode::RateLimited
                | ErrorCode::ExternalService
        )
    }

    /// Canonical HTTP status code for this error.
    pub fn http_status(self) -> http::StatusCode {
        match self {
            ErrorCode::ServiceUnavailable => http::StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ConnectionFailed => http::StatusCode::BAD_GATEWAY,
            ErrorCode::Timeout => http::StatusCode::GATEWAY_TIMEOUT,
            ErrorCode::RateLimited => http::StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::NotFound => http::StatusCode::NOT_FOUND,
            ErrorCode::AlreadyExists => http::StatusCode::CONFLICT,
            ErrorCode::Conflict => http::StatusCode::CONFLICT,
            ErrorCode::InvalidInput | ErrorCode::MissingField | ErrorCode::InvalidFormat => {
                http::StatusCode::UNPROCESSABLE_ENTITY
            }
            ErrorCode::Unauthorized => http::StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => http::StatusCode::FORBIDDEN,
            ErrorCode::TokenExpired | ErrorCode::InvalidToken => http::StatusCode::UNAUTHORIZED,
            ErrorCode::Internal | ErrorCode::DatabaseError | ErrorCode::ExternalService => {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }
            // 499 Client Closed Request (non-standard but widely used for cancellation)
            ErrorCode::Cancelled => {
                http::StatusCode::from_u16(499).unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR)
            }
            #[allow(unreachable_patterns)]
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable string representation (for logging / serialisation).
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            ErrorCode::ConnectionFailed => "CONNECTION_FAILED",
            ErrorCode::Timeout => "TIMEOUT",
            ErrorCode::RateLimited => "RATE_LIMITED",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::AlreadyExists => "ALREADY_EXISTS",
            ErrorCode::Conflict => "CONFLICT",
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::MissingField => "MISSING_FIELD",
            ErrorCode::InvalidFormat => "INVALID_FORMAT",
            ErrorCode::Unauthorized => "UNAUTHORIZED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::TokenExpired => "TOKEN_EXPIRED",
            ErrorCode::InvalidToken => "INVALID_TOKEN",
            ErrorCode::Internal => "INTERNAL",
            ErrorCode::DatabaseError => "DATABASE_ERROR",
            ErrorCode::ExternalService => "EXTERNAL_SERVICE",
            ErrorCode::Cancelled => "CANCELLED",
            #[allow(unreachable_patterns)]
            _ => "UNKNOWN",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_retryable ─────────────────────────────────────────────────────────

    #[test]
    fn retryable_service_unavailable() {
        assert!(ErrorCode::ServiceUnavailable.is_retryable());
    }

    #[test]
    fn retryable_connection_failed() {
        assert!(ErrorCode::ConnectionFailed.is_retryable());
    }

    #[test]
    fn retryable_timeout() {
        assert!(ErrorCode::Timeout.is_retryable());
    }

    #[test]
    fn retryable_rate_limited() {
        assert!(ErrorCode::RateLimited.is_retryable());
    }

    #[test]
    fn retryable_external_service() {
        assert!(ErrorCode::ExternalService.is_retryable());
    }

    #[test]
    fn not_retryable_not_found() {
        assert!(!ErrorCode::NotFound.is_retryable());
    }

    #[test]
    fn not_retryable_already_exists() {
        assert!(!ErrorCode::AlreadyExists.is_retryable());
    }

    #[test]
    fn not_retryable_conflict() {
        assert!(!ErrorCode::Conflict.is_retryable());
    }

    #[test]
    fn not_retryable_invalid_input() {
        assert!(!ErrorCode::InvalidInput.is_retryable());
    }

    #[test]
    fn not_retryable_missing_field() {
        assert!(!ErrorCode::MissingField.is_retryable());
    }

    #[test]
    fn not_retryable_invalid_format() {
        assert!(!ErrorCode::InvalidFormat.is_retryable());
    }

    #[test]
    fn not_retryable_unauthorized() {
        assert!(!ErrorCode::Unauthorized.is_retryable());
    }

    #[test]
    fn not_retryable_forbidden() {
        assert!(!ErrorCode::Forbidden.is_retryable());
    }

    #[test]
    fn not_retryable_token_expired() {
        assert!(!ErrorCode::TokenExpired.is_retryable());
    }

    #[test]
    fn not_retryable_invalid_token() {
        assert!(!ErrorCode::InvalidToken.is_retryable());
    }

    #[test]
    fn not_retryable_internal() {
        assert!(!ErrorCode::Internal.is_retryable());
    }

    #[test]
    fn not_retryable_database_error() {
        assert!(!ErrorCode::DatabaseError.is_retryable());
    }

    // ── http_status ───────────────────────────────────────────────────────────

    #[test]
    fn http_status_not_found_is_404() {
        assert_eq!(
            ErrorCode::NotFound.http_status(),
            http::StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn http_status_unauthorized_is_401() {
        assert_eq!(
            ErrorCode::Unauthorized.http_status(),
            http::StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn http_status_internal_is_500() {
        assert_eq!(
            ErrorCode::Internal.http_status(),
            http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn http_status_rate_limited_is_429() {
        assert_eq!(
            ErrorCode::RateLimited.http_status(),
            http::StatusCode::TOO_MANY_REQUESTS
        );
    }

    // ── as_str ────────────────────────────────────────────────────────────────

    #[test]
    fn as_str_connection_failed() {
        assert_eq!(ErrorCode::ConnectionFailed.as_str(), "CONNECTION_FAILED");
    }

    #[test]
    fn as_str_not_found() {
        assert_eq!(ErrorCode::NotFound.as_str(), "NOT_FOUND");
    }

    #[test]
    fn as_str_unauthorized() {
        assert_eq!(ErrorCode::Unauthorized.as_str(), "UNAUTHORIZED");
    }

    #[test]
    fn as_str_internal() {
        assert_eq!(ErrorCode::Internal.as_str(), "INTERNAL");
    }

    #[test]
    fn display_uses_as_str() {
        assert_eq!(format!("{}", ErrorCode::Timeout), "TIMEOUT");
    }
}

/// Machine-readable error code.
///
/// Defined as an enum so downstream code gets exhaustive match checking.
/// `#[non_exhaustive]` allows new variants to be added without a SemVer break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ErrorCode {
    // ── Connection / availability (all retryable) ──────────────────────
    ServiceUnavailable,
    ConnectionFailed,
    Timeout,
    RateLimited,

    // ── Resource ───────────────────────────────────────────────────────
    NotFound,
    AlreadyExists,
    Conflict,

    // ── Validation ─────────────────────────────────────────────────────
    InvalidInput,
    MissingField,
    InvalidFormat,

    // ── Auth ───────────────────────────────────────────────────────────
    Unauthorized,
    Forbidden,
    TokenExpired,
    InvalidToken,

    // ── Internal ───────────────────────────────────────────────────────
    Internal,
    DatabaseError,
    ExternalService,
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

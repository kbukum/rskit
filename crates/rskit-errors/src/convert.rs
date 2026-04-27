use crate::response::ProblemDetail;
use crate::{AppError, ErrorCode};

// ── std::io::Error ──────────────────────────────────────────────────────────

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(ErrorCode::Internal, e.to_string())
    }
}

// ── serde_json::Error ───────────────────────────────────────────────────────

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(ErrorCode::InvalidFormat, e.to_string())
    }
}

// ── std::fmt::Error ─────────────────────────────────────────────────────────

impl From<std::fmt::Error> for AppError {
    fn from(e: std::fmt::Error) -> Self {
        AppError::new(ErrorCode::Internal, e.to_string())
    }
}

// ── std::str::Utf8Error ─────────────────────────────────────────────────────

impl From<std::str::Utf8Error> for AppError {
    fn from(e: std::str::Utf8Error) -> Self {
        AppError::new(ErrorCode::InvalidInput, e.to_string())
    }
}

// ── http::StatusCode ────────────────────────────────────────────────────────

impl From<&AppError> for http::StatusCode {
    fn from(e: &AppError) -> Self {
        e.http_status
    }
}

// ── tonic::Status ───────────────────────────────────────────────────────────

impl From<AppError> for tonic::Status {
    fn from(e: AppError) -> Self {
        let code = match e.code {
            ErrorCode::ServiceUnavailable => tonic::Code::Unavailable,
            ErrorCode::ConnectionFailed => tonic::Code::Unavailable,
            ErrorCode::Timeout => tonic::Code::DeadlineExceeded,
            ErrorCode::RateLimited => tonic::Code::ResourceExhausted,
            ErrorCode::NotFound => tonic::Code::NotFound,
            ErrorCode::AlreadyExists => tonic::Code::AlreadyExists,
            ErrorCode::Conflict => tonic::Code::Aborted,
            ErrorCode::InvalidInput | ErrorCode::MissingField | ErrorCode::InvalidFormat => {
                tonic::Code::InvalidArgument
            }
            ErrorCode::Unauthorized | ErrorCode::TokenExpired | ErrorCode::InvalidToken => {
                tonic::Code::Unauthenticated
            }
            ErrorCode::Forbidden => tonic::Code::PermissionDenied,
            ErrorCode::Internal | ErrorCode::DatabaseError | ErrorCode::ExternalService => {
                tonic::Code::Internal
            }
            #[allow(unreachable_patterns)]
            _ => tonic::Code::Unknown,
        };
        let message = e.message.clone();

        // Encode RFC 9457 ProblemDetail as JSON bytes in the status details
        // field so that cross-service callers can recover the full AppError.
        if let Ok(json_bytes) = serde_json::to_vec(&ProblemDetail::from(&e)) {
            tonic::Status::with_details(code, message, json_bytes.into())
        } else {
            tonic::Status::new(code, message)
        }
    }
}

impl From<tonic::Status> for AppError {
    fn from(s: tonic::Status) -> Self {
        // Try to recover structured error details from the status details bytes.
        if !s.details().is_empty()
            && let Ok(pd) = serde_json::from_slice::<ProblemDetail>(s.details())
        {
            return AppError::new(pd.code, pd.detail)
                .retryable(pd.retryable)
                .with_details(pd.details);
        }

        // Fallback: map gRPC code to AppError code.
        let code = match s.code() {
            tonic::Code::Unavailable => ErrorCode::ServiceUnavailable,
            tonic::Code::DeadlineExceeded => ErrorCode::Timeout,
            tonic::Code::ResourceExhausted => ErrorCode::RateLimited,
            tonic::Code::NotFound => ErrorCode::NotFound,
            tonic::Code::AlreadyExists => ErrorCode::AlreadyExists,
            tonic::Code::Aborted => ErrorCode::Conflict,
            tonic::Code::InvalidArgument => ErrorCode::InvalidInput,
            tonic::Code::Unauthenticated => ErrorCode::Unauthorized,
            tonic::Code::PermissionDenied => ErrorCode::Forbidden,
            _ => ErrorCode::ExternalService,
        };
        AppError::new(code, s.message().to_string())
    }
}

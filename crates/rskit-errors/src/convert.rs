use crate::{AppError, ErrorCode};

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
            _ => tonic::Code::Unknown,
        };
        tonic::Status::new(code, e.message)
    }
}

impl From<tonic::Status> for AppError {
    fn from(s: tonic::Status) -> Self {
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

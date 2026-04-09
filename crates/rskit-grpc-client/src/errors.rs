use rskit_errors::{AppError, ErrorCode};
use tonic::Code;

/// Convert a tonic [`Status`] to an [`AppError`].
///
/// Maps gRPC status codes to rskit error codes with appropriate HTTP status
/// and human-readable messages.
pub fn status_to_app_error(status: tonic::Status) -> AppError {
    let code = status.code();
    let message = status.message();

    match code {
        Code::Ok => {
            // OK should never be an error, but handle gracefully
            AppError::new(ErrorCode::Internal, "unexpected OK status in error context")
        }

        Code::Cancelled => AppError::new(
            ErrorCode::Internal,
            format!("request cancelled{}", format_message(message)),
        ),

        Code::Unknown => AppError::new(ErrorCode::Internal, format_message(message)),

        Code::InvalidArgument => AppError::new(
            ErrorCode::InvalidInput,
            format!("invalid argument{}", format_message(message)),
        ),

        Code::DeadlineExceeded => {
            // Check if it's a connection timeout vs request timeout
            if is_connection_error_message(message) {
                AppError::new(
                    ErrorCode::ConnectionFailed,
                    format!("connection timeout{}", format_message(message)),
                )
            } else {
                AppError::new(
                    ErrorCode::Timeout,
                    format!("request deadline exceeded{}", format_message(message)),
                )
            }
        }

        Code::NotFound => AppError::new(
            ErrorCode::NotFound,
            format!("resource not found{}", format_message(message)),
        ),

        Code::AlreadyExists => AppError::new(
            ErrorCode::AlreadyExists,
            format!("resource already exists{}", format_message(message)),
        ),

        Code::PermissionDenied => AppError::new(
            ErrorCode::Forbidden,
            format!("permission denied{}", format_message(message)),
        ),

        Code::ResourceExhausted => AppError::new(
            ErrorCode::RateLimited,
            format!(
                "resource exhausted (rate limited){}",
                format_message(message)
            ),
        ),

        Code::FailedPrecondition => AppError::new(
            ErrorCode::Conflict,
            format!("failed precondition{}", format_message(message)),
        ),

        Code::Aborted => AppError::new(
            ErrorCode::Conflict,
            format!("operation aborted{}", format_message(message)),
        )
        .retryable(true),

        Code::OutOfRange => AppError::new(
            ErrorCode::InvalidInput,
            format!("out of range{}", format_message(message)),
        ),

        Code::Unimplemented => AppError::new(
            ErrorCode::Internal,
            format!("operation not implemented{}", format_message(message)),
        ),

        Code::Internal => AppError::new(
            ErrorCode::Internal,
            format!("internal server error{}", format_message(message)),
        ),

        Code::Unavailable => {
            // Check for connection errors
            if is_connection_error_message(message) {
                AppError::new(
                    ErrorCode::ConnectionFailed,
                    format!(
                        "service unavailable (connection failed){}",
                        format_message(message)
                    ),
                )
            } else {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("service unavailable{}", format_message(message)),
                )
            }
        }

        Code::DataLoss => AppError::new(
            ErrorCode::Internal,
            format!("data loss{}", format_message(message)),
        ),

        Code::Unauthenticated => AppError::new(
            ErrorCode::Unauthorized,
            format!(
                "unauthenticated (missing or invalid credentials){}",
                format_message(message)
            ),
        ),
    }
    .with_cause(status)
}

/// Convert an [`AppError`] to a tonic [`Status`].
///
/// Maps rskit error codes to appropriate gRPC status codes.
pub fn app_error_to_status(err: &AppError) -> tonic::Status {
    let code = match err.code {
        ErrorCode::NotFound => Code::NotFound,
        ErrorCode::AlreadyExists => Code::AlreadyExists,
        ErrorCode::InvalidInput | ErrorCode::MissingField | ErrorCode::InvalidFormat => {
            Code::InvalidArgument
        }
        ErrorCode::Unauthorized | ErrorCode::TokenExpired | ErrorCode::InvalidToken => {
            Code::Unauthenticated
        }
        ErrorCode::Forbidden => Code::PermissionDenied,
        ErrorCode::Conflict => Code::FailedPrecondition,
        ErrorCode::Timeout => Code::DeadlineExceeded,
        ErrorCode::RateLimited => Code::ResourceExhausted,
        ErrorCode::ServiceUnavailable | ErrorCode::ConnectionFailed => Code::Unavailable,
        ErrorCode::DatabaseError | ErrorCode::ExternalService | ErrorCode::Internal => {
            Code::Internal
        }
        _ => Code::Internal,
    };

    tonic::Status::new(code, err.message.clone())
}

/// Format a tonic error message for display.
fn format_message(msg: &str) -> String {
    if msg.is_empty() {
        String::new()
    } else {
        format!(": {}", msg)
    }
}

/// Check if an error message indicates a connection-level failure.
fn is_connection_error_message(msg: &str) -> bool {
    let patterns = [
        "connection refused",
        "connection reset",
        "no such host",
        "transport is closing",
        "connection closed",
        "unable to connect",
        "name resolution failed",
        "dns error",
    ];

    let msg_lower = msg.to_lowercase();
    patterns.iter().any(|p| msg_lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_to_error_not_found() {
        let status = tonic::Status::not_found("user not found");
        let err = status_to_app_error(status);
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn test_status_to_error_invalid_argument() {
        let status = tonic::Status::invalid_argument("invalid request");
        let err = status_to_app_error(status);
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_status_to_error_unavailable() {
        let status = tonic::Status::unavailable("service down");
        let err = status_to_app_error(status);
        assert_eq!(err.code, ErrorCode::ServiceUnavailable);
    }

    #[test]
    fn test_status_to_error_unauthenticated() {
        let status = tonic::Status::unauthenticated("invalid token");
        let err = status_to_app_error(status);
        assert_eq!(err.code, ErrorCode::Unauthorized);
    }

    #[test]
    fn test_app_error_to_status_not_found() {
        let err = AppError::new(ErrorCode::NotFound, "user not found");
        let status = app_error_to_status(&err);
        assert_eq!(status.code(), Code::NotFound);
    }

    #[test]
    fn test_app_error_to_status_invalid_input() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad request");
        let status = app_error_to_status(&err);
        assert_eq!(status.code(), Code::InvalidArgument);
    }

    #[test]
    fn test_app_error_to_status_unauthorized() {
        let err = AppError::new(ErrorCode::Unauthorized, "missing token");
        let status = app_error_to_status(&err);
        assert_eq!(status.code(), Code::Unauthenticated);
    }

    #[test]
    fn test_connection_error_detection() {
        assert!(is_connection_error_message("connection refused"));
        assert!(is_connection_error_message("Connection Reset"));
        assert!(is_connection_error_message("dns error"));
        assert!(!is_connection_error_message("request timeout"));
    }
}

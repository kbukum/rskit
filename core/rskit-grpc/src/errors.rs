use rskit_errors::{AppError, ErrorCode, ProblemDetail};
use tonic::Status;

/// Convert a tonic [`tonic::Status`] to an [`AppError`].
///
/// Maps gRPC status codes to rskit error codes with appropriate HTTP status
/// and human-readable messages.
pub fn status_to_app_error(status: Status) -> AppError {
    if !status.details().is_empty()
        && let Ok(problem) = serde_json::from_slice::<ProblemDetail>(status.details())
    {
        return AppError::new(problem.code, problem.detail)
            .retryable(problem.retryable)
            .with_details(problem.details);
    }

    AppError::new(
        grpc_code_to_error_code(status.code()),
        status.message().to_string(),
    )
}

/// Convert an [`AppError`] to a tonic [`tonic::Status`].
///
/// Maps rskit error codes to appropriate gRPC status codes.
pub fn app_error_to_status(err: &AppError) -> Status {
    let problem = ProblemDetail::from(err);
    if let Ok(json_bytes) = serde_json::to_vec(&problem) {
        Status::with_details(
            error_code_to_grpc_code(err.code),
            err.message.clone(),
            json_bytes.into(),
        )
    } else {
        Status::new(error_code_to_grpc_code(err.code), err.message.clone())
    }
}

/// Map an rskit [`ErrorCode`] to its canonical gRPC [`tonic::Code`].
#[must_use]
pub fn error_code_to_grpc_code(code: ErrorCode) -> tonic::Code {
    match code {
        ErrorCode::ServiceUnavailable | ErrorCode::ConnectionFailed => tonic::Code::Unavailable,
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
        ErrorCode::Cancelled => tonic::Code::Cancelled,
        _ => tonic::Code::Unknown,
    }
}

/// Map a gRPC [`tonic::Code`] to its canonical rskit [`ErrorCode`].
#[must_use]
pub fn grpc_code_to_error_code(code: tonic::Code) -> ErrorCode {
    match code {
        tonic::Code::Unavailable => ErrorCode::ServiceUnavailable,
        tonic::Code::DeadlineExceeded => ErrorCode::Timeout,
        tonic::Code::ResourceExhausted => ErrorCode::RateLimited,
        tonic::Code::NotFound => ErrorCode::NotFound,
        tonic::Code::AlreadyExists => ErrorCode::AlreadyExists,
        tonic::Code::Aborted | tonic::Code::FailedPrecondition => ErrorCode::Conflict,
        tonic::Code::InvalidArgument | tonic::Code::OutOfRange => ErrorCode::InvalidInput,
        tonic::Code::Unauthenticated => ErrorCode::Unauthorized,
        tonic::Code::PermissionDenied => ErrorCode::Forbidden,
        tonic::Code::Cancelled => ErrorCode::Cancelled,
        tonic::Code::Internal | tonic::Code::Unknown | tonic::Code::Unimplemented => {
            ErrorCode::Internal
        }
        tonic::Code::DataLoss => ErrorCode::ExternalService,
        _ => ErrorCode::ExternalService,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_to_grpc_code_covers_known_error_codes() {
        let cases = [
            (ErrorCode::ServiceUnavailable, tonic::Code::Unavailable),
            (ErrorCode::ConnectionFailed, tonic::Code::Unavailable),
            (ErrorCode::Timeout, tonic::Code::DeadlineExceeded),
            (ErrorCode::RateLimited, tonic::Code::ResourceExhausted),
            (ErrorCode::NotFound, tonic::Code::NotFound),
            (ErrorCode::AlreadyExists, tonic::Code::AlreadyExists),
            (ErrorCode::Conflict, tonic::Code::Aborted),
            (ErrorCode::InvalidInput, tonic::Code::InvalidArgument),
            (ErrorCode::MissingField, tonic::Code::InvalidArgument),
            (ErrorCode::InvalidFormat, tonic::Code::InvalidArgument),
            (ErrorCode::Unauthorized, tonic::Code::Unauthenticated),
            (ErrorCode::Forbidden, tonic::Code::PermissionDenied),
            (ErrorCode::TokenExpired, tonic::Code::Unauthenticated),
            (ErrorCode::InvalidToken, tonic::Code::Unauthenticated),
            (ErrorCode::Internal, tonic::Code::Internal),
            (ErrorCode::DatabaseError, tonic::Code::Internal),
            (ErrorCode::ExternalService, tonic::Code::Internal),
            (ErrorCode::Cancelled, tonic::Code::Cancelled),
        ];

        for (error_code, grpc_code) in cases {
            assert_eq!(error_code_to_grpc_code(error_code), grpc_code);
        }
    }

    #[test]
    fn grpc_code_to_error_code_covers_all_tonic_codes() {
        let cases = [
            (tonic::Code::Ok, ErrorCode::ExternalService),
            (tonic::Code::Cancelled, ErrorCode::Cancelled),
            (tonic::Code::Unknown, ErrorCode::Internal),
            (tonic::Code::InvalidArgument, ErrorCode::InvalidInput),
            (tonic::Code::DeadlineExceeded, ErrorCode::Timeout),
            (tonic::Code::NotFound, ErrorCode::NotFound),
            (tonic::Code::AlreadyExists, ErrorCode::AlreadyExists),
            (tonic::Code::PermissionDenied, ErrorCode::Forbidden),
            (tonic::Code::ResourceExhausted, ErrorCode::RateLimited),
            (tonic::Code::FailedPrecondition, ErrorCode::Conflict),
            (tonic::Code::Aborted, ErrorCode::Conflict),
            (tonic::Code::OutOfRange, ErrorCode::InvalidInput),
            (tonic::Code::Unimplemented, ErrorCode::Internal),
            (tonic::Code::Internal, ErrorCode::Internal),
            (tonic::Code::Unavailable, ErrorCode::ServiceUnavailable),
            (tonic::Code::DataLoss, ErrorCode::ExternalService),
            (tonic::Code::Unauthenticated, ErrorCode::Unauthorized),
        ];

        for (grpc_code, error_code) in cases {
            assert_eq!(grpc_code_to_error_code(grpc_code), error_code);
        }
    }

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
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_app_error_to_status_invalid_input() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad request");
        let status = app_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_app_error_to_status_unauthorized() {
        let err = AppError::new(ErrorCode::Unauthorized, "missing token");
        let status = app_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_status_to_error_cancelled_uses_canonical_errors_mapping() {
        let err = status_to_app_error(tonic::Status::cancelled("client cancelled"));
        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    #[test]
    fn app_error_to_status_preserves_problem_details() {
        let err = AppError::new(ErrorCode::NotFound, "user 42 not found").with_detail("id", "42");

        let status = app_error_to_status(&err);
        let recovered = status_to_app_error(status);

        assert_eq!(recovered.code, ErrorCode::NotFound);
        assert_eq!(recovered.message, "user 42 not found");
        assert_eq!(
            recovered.details.get("id").and_then(|value| value.as_str()),
            Some("42")
        );
    }
}

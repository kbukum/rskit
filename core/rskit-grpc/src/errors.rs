use rskit_errors::AppError;
use tonic::Status;

/// Convert a tonic [`tonic::Status`] to an [`AppError`].
///
/// Maps gRPC status codes to rskit error codes with appropriate HTTP status
/// and human-readable messages.
pub fn status_to_app_error(status: Status) -> AppError {
    status.into()
}

/// Convert an [`AppError`] to a tonic [`tonic::Status`].
///
/// Maps rskit error codes to appropriate gRPC status codes.
pub fn app_error_to_status(err: &AppError) -> Status {
    AppError::new(err.code, err.message.clone())
        .retryable(err.retryable)
        .with_details(err.details.clone())
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

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
}

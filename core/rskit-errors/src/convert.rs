use crate::{AppError, ErrorCode};

// ── std::io::Error ──────────────────────────────────────────────────────────

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind;
        // Map common kinds to their semantic code so HTTP status and retry
        // hints stay meaningful (e.g. a missing file is 404, not 500).
        let code = match e.kind() {
            ErrorKind::NotFound => ErrorCode::NotFound,
            ErrorKind::PermissionDenied => ErrorCode::Forbidden,
            ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
            ErrorKind::TimedOut => ErrorCode::Timeout,
            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe => ErrorCode::ConnectionFailed,
            ErrorKind::InvalidInput | ErrorKind::InvalidData => ErrorCode::InvalidInput,
            _ => ErrorCode::Internal,
        };
        let message = e.to_string();
        AppError::new(code, message).with_cause(e)
    }
}

// ── serde_json::Error ───────────────────────────────────────────────────────

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        let message = e.to_string();
        AppError::new(ErrorCode::InvalidFormat, message).with_cause(e)
    }
}

// ── std::fmt::Error ─────────────────────────────────────────────────────────

impl From<std::fmt::Error> for AppError {
    fn from(e: std::fmt::Error) -> Self {
        AppError::new(ErrorCode::Internal, e.to_string()).with_cause(e)
    }
}

// ── std::str::Utf8Error ─────────────────────────────────────────────────────

impl From<std::str::Utf8Error> for AppError {
    fn from(e: std::str::Utf8Error) -> Self {
        let message = e.to_string();
        AppError::new(ErrorCode::InvalidInput, message).with_cause(e)
    }
}

// ── http::StatusCode ────────────────────────────────────────────────────────

impl From<&AppError> for http::StatusCode {
    fn from(e: &AppError) -> Self {
        e.http_status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_not_found_maps_to_not_found() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = AppError::from(io);
        assert_eq!(err.code(), ErrorCode::NotFound);
        assert_eq!(err.http_status(), http::StatusCode::NOT_FOUND);
        assert!(err.cause().is_some());
    }

    #[test]
    fn io_permission_denied_maps_to_forbidden() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = AppError::from(io);
        assert_eq!(err.code(), ErrorCode::Forbidden);
    }

    #[test]
    fn io_timed_out_maps_to_timeout_and_is_retryable() {
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "slow");
        let err = AppError::from(io);
        assert_eq!(err.code(), ErrorCode::Timeout);
        assert!(err.is_retryable());
    }

    #[test]
    fn io_connection_refused_maps_to_connection_failed() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = AppError::from(io);
        assert_eq!(err.code(), ErrorCode::ConnectionFailed);
    }

    #[test]
    fn io_other_maps_to_internal() {
        let io = std::io::Error::other("weird");
        let err = AppError::from(io);
        assert_eq!(err.code(), ErrorCode::Internal);
    }

    #[test]
    fn serde_json_error_maps_to_invalid_format_with_cause() {
        let e = serde_json::from_str::<i32>("not json").unwrap_err();
        let err = AppError::from(e);
        assert_eq!(err.code(), ErrorCode::InvalidFormat);
        assert!(err.cause().is_some());
    }
}

use rskit_errors::{AppError, AppResult, ErrorCode};

#[test]
fn not_found_has_correct_code_and_status() {
    let err = AppError::not_found("user", Some("42"));
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.http_status.as_u16(), 404);
    assert!(!err.is_retryable());
}

#[test]
fn internal_error_is_retryable() {
    let err = AppError::new(ErrorCode::ServiceUnavailable, "boom");
    assert!(err.is_retryable());
}

#[test]
fn error_with_detail_and_cause_roundtrip() {
    let cause = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let err = AppError::timeout("db")
        .with_detail("query", "SELECT 1")
        .with_cause(cause);
    assert_eq!(err.code, ErrorCode::Timeout);
    assert!(err.cause.is_some());
    assert_eq!(err.details.get("query").and_then(|s| s.as_str()), Some("SELECT 1"));
}

#[test]
fn app_result_ok_passes_through() {
    fn ok_fn() -> AppResult<u32> { Ok(42) }
    assert_eq!(ok_fn().unwrap(), 42);
}

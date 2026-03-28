use rskit_errors::{AppError, ErrorCode};
use rskit_testutil::{assert_err_code, assert_ok, MockProvider};

// ── Assertion helpers ───────────────────────────────────────────────

#[test]
fn assert_ok_with_ok_result() {
    let result: Result<i32, AppError> = Ok(42);
    let value = assert_ok(result);

    assert_eq!(value, 42);
}

#[test]
fn assert_err_code_with_matching_error_code() {
    let result: Result<i32, AppError> = Err(AppError::not_found("widget", None));
    assert_err_code(result, ErrorCode::NotFound);
}

// ── MockProvider ────────────────────────────────────────────────────

#[test]
fn mock_provider_returns_configured_response() {
    let mock = MockProvider::<String, u64>::new();
    mock.will_return(99);

    let output = mock.execute("hello".to_string()).unwrap();

    assert_eq!(output, 99);
    assert_eq!(mock.call_count(), 1);
    assert_eq!(mock.calls(), vec!["hello".to_string()]);
}

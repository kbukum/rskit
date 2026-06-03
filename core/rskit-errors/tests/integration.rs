use std::collections::HashMap;
use std::error::Error;

use rskit_errors::{AppError, AppResult, ErrorCode, ProblemDetail};

// ═══════════════════════════════════════════════════════════════════════════
// 1. ErrorCode → HTTP status mapping (ALL 17 codes)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn http_status_all_codes_exhaustive() {
    let cases: Vec<(ErrorCode, u16)> = vec![
        (ErrorCode::ServiceUnavailable, 503),
        (ErrorCode::ConnectionFailed, 502),
        (ErrorCode::Timeout, 504),
        (ErrorCode::RateLimited, 429),
        (ErrorCode::NotFound, 404),
        (ErrorCode::AlreadyExists, 409),
        (ErrorCode::Conflict, 409),
        (ErrorCode::InvalidInput, 422),
        (ErrorCode::MissingField, 422),
        (ErrorCode::InvalidFormat, 422),
        (ErrorCode::Unauthorized, 401),
        (ErrorCode::Forbidden, 403),
        (ErrorCode::TokenExpired, 401),
        (ErrorCode::InvalidToken, 401),
        (ErrorCode::Internal, 500),
        (ErrorCode::DatabaseError, 500),
        (ErrorCode::ExternalService, 502),
        (ErrorCode::Cancelled, 408),
    ];
    for (code, expected) in cases {
        assert_eq!(
            code.http_status().as_u16(),
            expected,
            "{:?} should map to HTTP {}",
            code,
            expected
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. is_retryable() for ALL codes
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn is_retryable_all_codes_exhaustive() {
    let retryable = vec![
        ErrorCode::ServiceUnavailable,
        ErrorCode::ConnectionFailed,
        ErrorCode::Timeout,
        ErrorCode::RateLimited,
        ErrorCode::ExternalService,
    ];
    let not_retryable = vec![
        ErrorCode::NotFound,
        ErrorCode::AlreadyExists,
        ErrorCode::Conflict,
        ErrorCode::InvalidInput,
        ErrorCode::MissingField,
        ErrorCode::InvalidFormat,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::TokenExpired,
        ErrorCode::InvalidToken,
        ErrorCode::Internal,
        ErrorCode::DatabaseError,
        ErrorCode::Cancelled,
    ];
    for code in retryable {
        assert!(code.is_retryable(), "{:?} should be retryable", code);
    }
    for code in not_retryable {
        assert!(!code.is_retryable(), "{:?} should NOT be retryable", code);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. AppError constructors — ALL convenience factory methods
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn not_found_has_correct_code_and_status() {
    let err = AppError::not_found("user", Some("42"));
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert_eq!(err.http_status().as_u16(), 404);
    assert!(!err.is_retryable());
    assert!(err.message().contains("user"));
    assert!(err.message().contains("42"));
}

#[test]
fn not_found_without_id() {
    let err = AppError::not_found("Order", None);
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert!(err.message().contains("Order"));
    assert!(!err.message().contains("'"));
}

#[test]
fn service_unavailable_constructor() {
    let err = AppError::service_unavailable("payment-api");
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    assert!(err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 503);
    assert!(err.message().contains("payment-api"));
}

#[test]
fn connection_failed_constructor() {
    let err = AppError::connection_failed("redis");
    assert_eq!(err.code(), ErrorCode::ConnectionFailed);
    assert!(err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 502);
    assert!(err.message().contains("redis"));
}

#[test]
fn timeout_constructor() {
    let err = AppError::timeout("db query");
    assert_eq!(err.code(), ErrorCode::Timeout);
    assert!(err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 504);
    assert!(err.message().contains("db query"));
}

#[test]
fn rate_limited_constructor() {
    let err = AppError::rate_limited();
    assert_eq!(err.code(), ErrorCode::RateLimited);
    assert!(err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 429);
}

#[test]
fn already_exists_constructor() {
    let err = AppError::already_exists("email");
    assert_eq!(err.code(), ErrorCode::AlreadyExists);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 409);
    assert!(err.message().contains("email"));
}

#[test]
fn conflict_constructor() {
    let err = AppError::conflict("version mismatch");
    assert_eq!(err.code(), ErrorCode::Conflict);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 409);
    assert!(err.message().contains("version mismatch"));
}

#[test]
fn invalid_input_constructor() {
    let err = AppError::invalid_input("email", "must contain @");
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 422);
    assert!(err.message().contains("email"));
    assert!(err.message().contains("must contain @"));
}

#[test]
fn missing_field_constructor() {
    let err = AppError::missing_field("username");
    assert_eq!(err.code(), ErrorCode::MissingField);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 422);
    assert!(err.message().contains("username"));
}

#[test]
fn invalid_format_constructor() {
    let err = AppError::invalid_format("date", "ISO 8601");
    assert_eq!(err.code(), ErrorCode::InvalidFormat);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 422);
    assert!(err.message().contains("date"));
    assert!(err.message().contains("ISO 8601"));
}

#[test]
fn unauthorized_constructor() {
    let err = AppError::unauthorized("missing bearer token");
    assert_eq!(err.code(), ErrorCode::Unauthorized);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 401);
    assert!(err.message().contains("missing bearer token"));
}

#[test]
fn forbidden_constructor() {
    let err = AppError::forbidden("admin only");
    assert_eq!(err.code(), ErrorCode::Forbidden);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 403);
    assert!(err.message().contains("admin only"));
}

#[test]
fn token_expired_constructor() {
    let err = AppError::token_expired();
    assert_eq!(err.code(), ErrorCode::TokenExpired);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 401);
    assert!(err.message().contains("expired"));
}

#[test]
fn invalid_token_constructor() {
    let err = AppError::invalid_token();
    assert_eq!(err.code(), ErrorCode::InvalidToken);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 401);
    assert!(err.message().contains("invalid"));
}

#[test]
fn internal_constructor_wraps_cause() {
    let cause = std::io::Error::other("disk full");
    let err = AppError::internal(cause);
    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 500);
    // message must be generic — must NOT expose the cause to callers
    assert_eq!(err.message(), "internal server error");
    assert!(
        !err.message().contains("disk full"),
        "cause leaked into message"
    );
    assert!(err.cause().is_some());
}

#[test]
fn database_error_constructor_wraps_cause() {
    let cause = std::io::Error::other("connection reset");
    let err = AppError::database_error(cause);
    assert_eq!(err.code(), ErrorCode::DatabaseError);
    assert!(!err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 500);
    // message must be generic — must NOT expose the cause to callers
    assert_eq!(err.message(), "database error");
    assert!(
        !err.message().contains("connection reset"),
        "cause leaked into message"
    );
    assert!(err.cause().is_some());
}

#[test]
fn external_service_constructor() {
    let cause = std::io::Error::other("500 Internal Server Error");
    let err = AppError::external_service("stripe", cause);
    assert_eq!(err.code(), ErrorCode::ExternalService);
    assert!(err.is_retryable());
    assert_eq!(err.http_status().as_u16(), 502);
    assert!(err.cause().is_some());
    assert_eq!(
        err.details().get("service").and_then(|v| v.as_str()),
        Some("stripe")
    );
}

#[test]
fn internal_wraps_arbitrary_error() {
    let cause = std::io::Error::other("oops");
    let err = AppError::internal(cause);
    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.cause().is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Builder pattern
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn with_cause_preserves_cause() {
    let cause = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let err = AppError::new(ErrorCode::Timeout, "timeout").with_cause(cause);
    assert!(err.cause().is_some());
    let source = err.source().unwrap();
    assert!(source.to_string().contains("timed out"));
}

#[test]
fn with_detail_adds_entry() {
    let err = AppError::new(ErrorCode::InvalidInput, "bad").with_detail("field", "name");
    assert_eq!(
        err.details().get("field").and_then(|v| v.as_str()),
        Some("name")
    );
}

#[test]
fn with_details_merges_map() {
    let mut details = HashMap::new();
    details.insert("a".to_string(), serde_json::json!("one"));
    details.insert("b".to_string(), serde_json::json!(2));

    let err = AppError::new(ErrorCode::Internal, "err").with_details(details);
    assert_eq!(err.details().len(), 2);
    assert_eq!(err.details().get("a").and_then(|v| v.as_str()), Some("one"));
    assert_eq!(err.details().get("b").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn with_details_merges_with_existing() {
    let mut extra = HashMap::new();
    extra.insert("y".to_string(), serde_json::json!("val"));

    let err = AppError::new(ErrorCode::Internal, "err")
        .with_detail("x", "existing")
        .with_details(extra);
    assert_eq!(err.details().len(), 2);
    assert!(err.details().contains_key("x"));
    assert!(err.details().contains_key("y"));
}

#[test]
fn retryable_override_true() {
    let err = AppError::new(ErrorCode::Internal, "err").retryable(true);
    assert!(err.is_retryable());
}

#[test]
fn retryable_override_false() {
    let err = AppError::new(ErrorCode::Timeout, "err").retryable(false);
    assert!(!err.is_retryable());
}

#[test]
fn chained_builders() {
    let cause = std::io::Error::other("root cause");
    let err = AppError::new(ErrorCode::ExternalService, "fail")
        .with_cause(cause)
        .with_detail("service", "api-x")
        .with_detail("endpoint", "/health")
        .retryable(false);

    assert_eq!(err.code(), ErrorCode::ExternalService);
    assert!(!err.is_retryable());
    assert!(err.cause().is_some());
    assert_eq!(err.details().len(), 2);
    assert_eq!(
        err.details().get("service").and_then(|v| v.as_str()),
        Some("api-x")
    );
    assert_eq!(
        err.details().get("endpoint").and_then(|v| v.as_str()),
        Some("/health")
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Display + Error trait
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn display_format_is_code_colon_message() {
    let err = AppError::new(ErrorCode::NotFound, "item missing");
    let display = format!("{err}");
    assert_eq!(display, "NOT_FOUND: item missing");
}

#[test]
fn display_format_for_various_codes() {
    let cases = vec![
        (ErrorCode::Timeout, "slow", "TIMEOUT: slow"),
        (ErrorCode::Forbidden, "nope", "FORBIDDEN: nope"),
        (ErrorCode::Internal, "oops", "INTERNAL_ERROR: oops"),
    ];
    for (code, msg, expected) in cases {
        let err = AppError::new(code, msg);
        assert_eq!(format!("{err}"), expected);
    }
}

#[test]
fn error_source_returns_none_without_cause() {
    let err = AppError::new(ErrorCode::Internal, "no cause");
    assert!(err.source().is_none());
}

#[test]
fn error_source_returns_cause() {
    let cause = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err = AppError::new(ErrorCode::Internal, "wrap").with_cause(cause);
    let src = err.source().unwrap();
    assert_eq!(src.to_string(), "file not found");
}

#[test]
fn error_trait_object_usable() {
    let err = AppError::new(ErrorCode::Internal, "generic");
    let boxed: Box<dyn Error> = Box::new(err);
    assert!(boxed.to_string().contains("INTERNAL"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. From conversions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn from_io_error_maps_kind_to_code() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
    let err: AppError = io_err.into();
    assert_eq!(err.code(), ErrorCode::Forbidden);
    assert!(err.message().contains("no access"));
}

#[test]
fn from_serde_json_error_maps_to_invalid_format() {
    let json_err = serde_json::from_str::<serde_json::Value>("not json {{{").unwrap_err();
    let err: AppError = json_err.into();
    assert_eq!(err.code(), ErrorCode::InvalidFormat);
    assert!(!err.message().is_empty());
}

#[test]
fn from_fmt_error_maps_to_internal() {
    let fmt_err = std::fmt::Error;
    let err: AppError = fmt_err.into();
    assert_eq!(err.code(), ErrorCode::Internal);
}

#[test]
fn from_app_error_ref_to_http_status() {
    let err = AppError::new(ErrorCode::Forbidden, "denied");
    let status: http::StatusCode = (&err).into();
    assert_eq!(status, http::StatusCode::FORBIDDEN);
}

#[test]
fn from_app_error_ref_to_http_status_all_codes() {
    let cases: Vec<(ErrorCode, http::StatusCode)> = vec![
        (ErrorCode::NotFound, http::StatusCode::NOT_FOUND),
        (ErrorCode::Unauthorized, http::StatusCode::UNAUTHORIZED),
        (ErrorCode::Internal, http::StatusCode::INTERNAL_SERVER_ERROR),
        (ErrorCode::RateLimited, http::StatusCode::TOO_MANY_REQUESTS),
        (ErrorCode::Timeout, http::StatusCode::GATEWAY_TIMEOUT),
    ];
    for (code, expected) in cases {
        let err = AppError::new(code, "test");
        let status: http::StatusCode = (&err).into();
        assert_eq!(status, expected, "{:?} ref conversion", code);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. AppResult type alias
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn app_result_ok_passes_through() {
    fn ok_fn() -> AppResult<u32> {
        Ok(42)
    }
    assert_eq!(ok_fn().unwrap(), 42);
}

#[test]
fn app_result_err_propagates_with_question_mark() {
    fn failing() -> AppResult<()> {
        Err(AppError::not_found("User", Some("99")))
    }
    fn caller() -> AppResult<String> {
        failing()?;
        Ok("unreachable".into())
    }
    let result = caller();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), ErrorCode::NotFound);
}

#[test]
fn app_result_question_mark_with_io_error() {
    fn may_fail() -> AppResult<String> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        Err(io_err)?;
        Ok("ok".into())
    }
    let result = may_fail();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), ErrorCode::NotFound);
}

#[test]
fn app_result_map_err() {
    let result: Result<i32, String> = Err("bad input".to_string());
    let app_result: AppResult<i32> = result.map_err(|msg| AppError::invalid_input("field", msg));
    assert!(app_result.is_err());
    let err = app_result.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.message().contains("bad input"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. ProblemDetail — serialization, deserialization, structure (RFC 9457)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn problem_detail_from_app_error_fields() {
    let err = AppError::not_found("User", Some("42"));
    let pd = ProblemDetail::from(&err);
    assert_eq!(pd.status, 404);
    assert_eq!(pd.title, "Not Found");
    assert!(pd.detail.contains("User"));
    assert!(pd.detail.contains("42"));
    assert!(pd.error_type.ends_with("not-found"));
    assert!(pd.instance.is_none());
    assert!(pd.details.is_empty());
    assert_eq!(pd.code, ErrorCode::NotFound);
    assert!(!pd.retryable);
}

#[test]
fn problem_detail_error_type_uri_format() {
    let cases: Vec<(ErrorCode, &str)> = vec![
        (ErrorCode::ServiceUnavailable, "service-unavailable"),
        (ErrorCode::ConnectionFailed, "connection-failed"),
        (ErrorCode::TokenExpired, "token-expired"),
        (ErrorCode::InvalidInput, "invalid-input"),
        (ErrorCode::DatabaseError, "database-error"),
        (ErrorCode::ExternalService, "external-service-error"),
    ];
    for (code, expected_slug) in cases {
        let err = AppError::new(code, "test");
        let pd = ProblemDetail::from(&err);
        assert!(
            pd.error_type.ends_with(expected_slug),
            "URI for {:?} was: {}",
            code,
            pd.error_type
        );
        assert!(
            pd.error_type.starts_with("https://"),
            "URI for {:?} was: {}",
            code,
            pd.error_type
        );
    }
}

#[test]
fn problem_detail_from_owned_app_error() {
    let err = AppError::unauthorized("no token");
    let pd = ProblemDetail::from(err);
    assert_eq!(pd.status, 401);
    assert_eq!(pd.detail, "no token");
    assert_eq!(pd.code, ErrorCode::Unauthorized);
}

#[test]
fn app_error_serialization_never_leaks_cause() {
    // S1: the underlying cause is for internal logging only and must never be
    // serialized into a wire response (neither directly nor via ProblemDetail).
    let cause = std::io::Error::other("secret connection string postgres://u:p@host");
    let err = AppError::internal(cause);

    let direct = serde_json::to_string(&err).unwrap();
    assert!(!direct.contains("postgres://"), "cause leaked: {direct}");
    assert!(
        !direct.contains("cause"),
        "cause field serialized: {direct}"
    );

    let pd = serde_json::to_string(&ProblemDetail::from(&err)).unwrap();
    assert!(
        !pd.contains("postgres://"),
        "cause leaked via problem detail: {pd}"
    );
    assert_eq!(err.message(), "internal server error");
}

#[test]
fn problem_detail_serializes_to_json() {
    let err = AppError::new(ErrorCode::NotFound, "item not found");
    let pd = ProblemDetail::from(&err);
    let json = serde_json::to_value(&pd).unwrap();

    assert!(
        json["type"].as_str().unwrap().ends_with("not-found"),
        "type: {}",
        json["type"]
    );
    assert_eq!(json["title"], "Not Found");
    assert_eq!(json["status"], 404);
    assert_eq!(json["detail"], "item not found");
    assert_eq!(json["code"], "NOT_FOUND");
    assert_eq!(json["retryable"], false);
    // instance is None → should be absent due to skip_serializing_if
    assert!(json.get("instance").is_none());
    // details is empty → should be absent due to skip_serializing_if
    assert!(json.get("details").is_none());
}

#[test]
fn problem_detail_json_roundtrip() {
    let err = AppError::new(ErrorCode::Forbidden, "not allowed");
    let pd = ProblemDetail::from(&err);
    let json_str = serde_json::to_string(&pd).unwrap();
    let back: ProblemDetail = serde_json::from_str(&json_str).unwrap();

    assert_eq!(back.status, pd.status);
    assert_eq!(back.title, pd.title);
    assert_eq!(back.detail, pd.detail);
    assert_eq!(back.error_type, pd.error_type);
    assert_eq!(back.code, pd.code);
    assert_eq!(back.retryable, pd.retryable);
}

#[test]
fn problem_detail_with_details_and_instance_roundtrip() {
    let mut pd = ProblemDetail::from(&AppError::new(ErrorCode::Internal, "err"));
    pd.details
        .insert("trace_id".to_string(), serde_json::json!("abc-123"));
    pd.instance = Some("/api/v1/users/42".to_string());

    let json_str = serde_json::to_string(&pd).unwrap();
    let back: ProblemDetail = serde_json::from_str(&json_str).unwrap();

    assert_eq!(
        back.details.get("trace_id").and_then(|v| v.as_str()),
        Some("abc-123")
    );
    assert_eq!(back.instance.as_deref(), Some("/api/v1/users/42"));
}

#[test]
fn problem_detail_deserialize_from_raw_json() {
    let raw = r#"{
        "type": "https://rskit.dev/errors/not-found",
        "title": "Not Found",
        "status": 404,
        "detail": "user not found",
        "code": "NOT_FOUND",
        "retryable": false
    }"#;
    let pd: ProblemDetail = serde_json::from_str(raw).unwrap();
    assert_eq!(pd.status, 404);
    assert_eq!(pd.title, "Not Found");
    assert_eq!(pd.detail, "user not found");
    assert!(pd.details.is_empty());
    assert!(pd.instance.is_none());
}

#[test]
fn problem_detail_title_is_title_cased() {
    let cases: &[(ErrorCode, &str)] = &[
        (ErrorCode::NotFound, "Not Found"),
        (ErrorCode::ServiceUnavailable, "Service Unavailable"),
        (ErrorCode::InvalidInput, "Invalid Input"),
        (ErrorCode::DatabaseError, "Database Error"),
        (ErrorCode::Internal, "Internal Error"),
        (ErrorCode::Unauthorized, "Unauthorized"),
    ];
    for (code, expected_title) in cases {
        let pd = ProblemDetail::from(&AppError::new(*code, "test"));
        assert_eq!(pd.title, *expected_title, "title for {:?}", code);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_string_message() {
    let err = AppError::new(ErrorCode::Internal, "");
    assert_eq!(err.message(), "");
    assert_eq!(format!("{err}"), "INTERNAL_ERROR: ");
}

#[test]
fn very_long_message() {
    let long_msg = "x".repeat(10_000);
    let err = AppError::new(ErrorCode::Internal, long_msg.clone());
    assert_eq!(err.message(), long_msg);
    assert_eq!(err.message().len(), 10_000);
}

#[test]
fn unicode_in_message() {
    let err = AppError::new(ErrorCode::InvalidInput, "名前が無効です 🚀");
    assert_eq!(err.message(), "名前が無効です 🚀");
    assert!(format!("{err}").contains("🚀"));
}

#[test]
fn unicode_in_details() {
    let err = AppError::new(ErrorCode::InvalidInput, "err")
        .with_detail("field", "名前")
        .with_detail("emoji", "🎉");
    assert_eq!(
        err.details().get("field").and_then(|v| v.as_str()),
        Some("名前")
    );
    assert_eq!(
        err.details().get("emoji").and_then(|v| v.as_str()),
        Some("🎉")
    );
}

#[test]
fn details_with_complex_json_values() {
    let err = AppError::new(ErrorCode::InvalidInput, "complex")
        .with_detail("array", serde_json::json!([1, 2, 3]))
        .with_detail("nested", serde_json::json!({"a": {"b": "c"}}))
        .with_detail("null_val", serde_json::Value::Null)
        .with_detail("bool_val", serde_json::json!(true))
        .with_detail("number", serde_json::json!(42.5));

    assert_eq!(err.details().len(), 5);
    assert!(err.details()["array"].is_array());
    assert!(err.details()["nested"].is_object());
    assert!(err.details()["null_val"].is_null());
    assert_eq!(err.details()["bool_val"].as_bool(), Some(true));
    assert_eq!(err.details()["number"].as_f64(), Some(42.5));
}

#[test]
fn error_code_serde_roundtrip() {
    let code = ErrorCode::ServiceUnavailable;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""SERVICE_UNAVAILABLE""#);
    let deserialized: ErrorCode = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, code);
}

#[test]
fn error_code_serde_all_variants() {
    let all_codes = vec![
        (ErrorCode::ServiceUnavailable, "SERVICE_UNAVAILABLE"),
        (ErrorCode::ConnectionFailed, "CONNECTION_FAILED"),
        (ErrorCode::Timeout, "TIMEOUT"),
        (ErrorCode::RateLimited, "RATE_LIMITED"),
        (ErrorCode::NotFound, "NOT_FOUND"),
        (ErrorCode::AlreadyExists, "ALREADY_EXISTS"),
        (ErrorCode::Conflict, "CONFLICT"),
        (ErrorCode::InvalidInput, "INVALID_INPUT"),
        (ErrorCode::MissingField, "MISSING_FIELD"),
        (ErrorCode::InvalidFormat, "INVALID_FORMAT"),
        (ErrorCode::Unauthorized, "UNAUTHORIZED"),
        (ErrorCode::Forbidden, "FORBIDDEN"),
        (ErrorCode::TokenExpired, "TOKEN_EXPIRED"),
        (ErrorCode::InvalidToken, "INVALID_TOKEN"),
        (ErrorCode::Internal, "INTERNAL_ERROR"),
        (ErrorCode::DatabaseError, "DATABASE_ERROR"),
        (ErrorCode::ExternalService, "EXTERNAL_SERVICE_ERROR"),
    ];
    for (code, expected_str) in all_codes {
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(
            json,
            format!("\"{}\"", expected_str),
            "serialize {:?}",
            code
        );
        let back: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, code, "deserialize {:?}", code);
    }
}

#[test]
fn error_code_as_str_all_variants() {
    let cases = vec![
        (ErrorCode::ServiceUnavailable, "SERVICE_UNAVAILABLE"),
        (ErrorCode::ConnectionFailed, "CONNECTION_FAILED"),
        (ErrorCode::Timeout, "TIMEOUT"),
        (ErrorCode::RateLimited, "RATE_LIMITED"),
        (ErrorCode::NotFound, "NOT_FOUND"),
        (ErrorCode::AlreadyExists, "ALREADY_EXISTS"),
        (ErrorCode::Conflict, "CONFLICT"),
        (ErrorCode::InvalidInput, "INVALID_INPUT"),
        (ErrorCode::MissingField, "MISSING_FIELD"),
        (ErrorCode::InvalidFormat, "INVALID_FORMAT"),
        (ErrorCode::Unauthorized, "UNAUTHORIZED"),
        (ErrorCode::Forbidden, "FORBIDDEN"),
        (ErrorCode::TokenExpired, "TOKEN_EXPIRED"),
        (ErrorCode::InvalidToken, "INVALID_TOKEN"),
        (ErrorCode::Internal, "INTERNAL_ERROR"),
        (ErrorCode::DatabaseError, "DATABASE_ERROR"),
        (ErrorCode::ExternalService, "EXTERNAL_SERVICE_ERROR"),
    ];
    for (code, expected) in cases {
        assert_eq!(code.as_str(), expected, "{:?}.as_str()", code);
    }
}

#[test]
fn error_code_display_matches_as_str() {
    let codes = vec![
        ErrorCode::ServiceUnavailable,
        ErrorCode::ConnectionFailed,
        ErrorCode::Timeout,
        ErrorCode::RateLimited,
        ErrorCode::NotFound,
        ErrorCode::AlreadyExists,
        ErrorCode::Conflict,
        ErrorCode::InvalidInput,
        ErrorCode::MissingField,
        ErrorCode::InvalidFormat,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::TokenExpired,
        ErrorCode::InvalidToken,
        ErrorCode::Internal,
        ErrorCode::DatabaseError,
        ErrorCode::ExternalService,
    ];
    for code in codes {
        assert_eq!(format!("{code}"), code.as_str(), "Display for {:?}", code);
    }
}

#[test]
fn error_code_clone_copy_eq_hash() {
    let code = ErrorCode::NotFound;
    let cloned = code;
    let copied = code;
    assert_eq!(code, cloned);
    assert_eq!(code, copied);

    // Usable as HashMap key
    let mut map = HashMap::new();
    map.insert(code, "found");
    assert_eq!(map.get(&ErrorCode::NotFound), Some(&"found"));
}

#[test]
fn app_error_serialize_without_details() {
    let err = AppError::new(ErrorCode::NotFound, "gone");
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "NOT_FOUND");
    assert_eq!(json["message"], "gone");
    assert_eq!(json["retryable"], false);
    // details should be absent when empty
    assert!(json.get("details").is_none());
}

#[test]
fn app_error_serialize_with_details() {
    let err = AppError::new(ErrorCode::InvalidInput, "bad").with_detail("field", "email");
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "INVALID_INPUT");
    assert!(json.get("details").is_some());
    assert_eq!(json["details"]["field"], "email");
}

#[test]
fn query_helper_is_not_found() {
    assert!(AppError::not_found("X", None).is_not_found());
    assert!(!AppError::unauthorized("x").is_not_found());
    assert!(!AppError::timeout("x").is_not_found());
}

#[test]
fn query_helper_is_unauthorized() {
    assert!(AppError::unauthorized("x").is_unauthorized());
    assert!(AppError::token_expired().is_unauthorized());
    assert!(AppError::invalid_token().is_unauthorized());
    assert!(!AppError::forbidden("x").is_unauthorized());
    assert!(!AppError::not_found("x", None).is_unauthorized());
}

#[test]
fn internal_error_is_not_retryable_by_default() {
    let err = AppError::new(ErrorCode::Internal, "boom");
    assert!(!err.is_retryable());
}

#[test]
fn service_unavailable_is_retryable_by_default() {
    let err = AppError::new(ErrorCode::ServiceUnavailable, "boom");
    assert!(err.is_retryable());
}

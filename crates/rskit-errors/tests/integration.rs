use std::collections::HashMap;
use std::error::Error;

use rskit_errors::{AppError, AppResult, ErrorCode, ErrorResponse};

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
        (ErrorCode::ExternalService, 500),
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
// 2. ErrorCode → gRPC code mapping (tonic integration)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn grpc_code_all_error_codes_exhaustive() {
    let cases: Vec<(ErrorCode, tonic::Code)> = vec![
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
    ];
    for (code, expected_grpc) in cases {
        let err = AppError::new(code, "test");
        let status: tonic::Status = err.into();
        assert_eq!(
            status.code(),
            expected_grpc,
            "{:?} should map to gRPC {:?}",
            code,
            expected_grpc
        );
    }
}

#[test]
fn grpc_status_preserves_message() {
    let err = AppError::new(ErrorCode::NotFound, "user 42 not found");
    let status: tonic::Status = err.into();
    assert_eq!(status.message(), "user 42 not found");
}

#[test]
fn grpc_status_to_app_error_roundtrip() {
    let cases: Vec<(tonic::Code, ErrorCode)> = vec![
        (tonic::Code::Unavailable, ErrorCode::ServiceUnavailable),
        (tonic::Code::DeadlineExceeded, ErrorCode::Timeout),
        (tonic::Code::ResourceExhausted, ErrorCode::RateLimited),
        (tonic::Code::NotFound, ErrorCode::NotFound),
        (tonic::Code::AlreadyExists, ErrorCode::AlreadyExists),
        (tonic::Code::Aborted, ErrorCode::Conflict),
        (tonic::Code::InvalidArgument, ErrorCode::InvalidInput),
        (tonic::Code::Unauthenticated, ErrorCode::Unauthorized),
        (tonic::Code::PermissionDenied, ErrorCode::Forbidden),
    ];
    for (grpc_code, expected_error_code) in cases {
        let status = tonic::Status::new(grpc_code, "test msg");
        let err: AppError = status.into();
        assert_eq!(
            err.code, expected_error_code,
            "gRPC {:?} should map to {:?}",
            grpc_code, expected_error_code
        );
        assert_eq!(err.message, "test msg");
    }
}

#[test]
fn grpc_unknown_code_maps_to_external_service() {
    let status = tonic::Status::new(tonic::Code::DataLoss, "data lost");
    let err: AppError = status.into();
    assert_eq!(err.code, ErrorCode::ExternalService);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. is_retryable() for ALL codes
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
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.http_status.as_u16(), 404);
    assert!(!err.is_retryable());
    assert!(err.message.contains("user"));
    assert!(err.message.contains("42"));
}

#[test]
fn not_found_without_id() {
    let err = AppError::not_found("Order", None);
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.message.contains("Order"));
    assert!(!err.message.contains("'"));
}

#[test]
fn service_unavailable_constructor() {
    let err = AppError::service_unavailable("payment-api");
    assert_eq!(err.code, ErrorCode::ServiceUnavailable);
    assert!(err.retryable);
    assert_eq!(err.http_status.as_u16(), 503);
    assert!(err.message.contains("payment-api"));
}

#[test]
fn connection_failed_constructor() {
    let err = AppError::connection_failed("redis");
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
    assert!(err.retryable);
    assert_eq!(err.http_status.as_u16(), 502);
    assert!(err.message.contains("redis"));
}

#[test]
fn timeout_constructor() {
    let err = AppError::timeout("db query");
    assert_eq!(err.code, ErrorCode::Timeout);
    assert!(err.retryable);
    assert_eq!(err.http_status.as_u16(), 504);
    assert!(err.message.contains("db query"));
}

#[test]
fn rate_limited_constructor() {
    let err = AppError::rate_limited();
    assert_eq!(err.code, ErrorCode::RateLimited);
    assert!(err.retryable);
    assert_eq!(err.http_status.as_u16(), 429);
}

#[test]
fn already_exists_constructor() {
    let err = AppError::already_exists("email");
    assert_eq!(err.code, ErrorCode::AlreadyExists);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 409);
    assert!(err.message.contains("email"));
}

#[test]
fn conflict_constructor() {
    let err = AppError::conflict("version mismatch");
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 409);
    assert!(err.message.contains("version mismatch"));
}

#[test]
fn invalid_input_constructor() {
    let err = AppError::invalid_input("email", "must contain @");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 422);
    assert!(err.message.contains("email"));
    assert!(err.message.contains("must contain @"));
}

#[test]
fn missing_field_constructor() {
    let err = AppError::missing_field("username");
    assert_eq!(err.code, ErrorCode::MissingField);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 422);
    assert!(err.message.contains("username"));
}

#[test]
fn invalid_format_constructor() {
    let err = AppError::invalid_format("date", "ISO 8601");
    assert_eq!(err.code, ErrorCode::InvalidFormat);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 422);
    assert!(err.message.contains("date"));
    assert!(err.message.contains("ISO 8601"));
}

#[test]
fn unauthorized_constructor() {
    let err = AppError::unauthorized("missing bearer token");
    assert_eq!(err.code, ErrorCode::Unauthorized);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 401);
    assert!(err.message.contains("missing bearer token"));
}

#[test]
fn forbidden_constructor() {
    let err = AppError::forbidden("admin only");
    assert_eq!(err.code, ErrorCode::Forbidden);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 403);
    assert!(err.message.contains("admin only"));
}

#[test]
fn token_expired_constructor() {
    let err = AppError::token_expired();
    assert_eq!(err.code, ErrorCode::TokenExpired);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 401);
    assert!(err.message.contains("expired"));
}

#[test]
fn invalid_token_constructor() {
    let err = AppError::invalid_token();
    assert_eq!(err.code, ErrorCode::InvalidToken);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 401);
    assert!(err.message.contains("invalid"));
}

#[test]
fn internal_constructor_wraps_cause() {
    let cause = std::io::Error::other("disk full");
    let err = AppError::internal(cause);
    assert_eq!(err.code, ErrorCode::Internal);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 500);
    assert!(err.message.contains("disk full"));
    assert!(err.cause.is_some());
}

#[test]
fn database_error_constructor_wraps_cause() {
    let cause = std::io::Error::other("connection reset");
    let err = AppError::database_error(cause);
    assert_eq!(err.code, ErrorCode::DatabaseError);
    assert!(!err.retryable);
    assert_eq!(err.http_status.as_u16(), 500);
    assert!(err.message.contains("database error"));
    assert!(err.message.contains("connection reset"));
    assert!(err.cause.is_some());
}

#[test]
fn external_service_constructor() {
    let cause = std::io::Error::other("500 Internal Server Error");
    let err = AppError::external_service("stripe", cause);
    assert_eq!(err.code, ErrorCode::ExternalService);
    assert!(err.retryable);
    assert_eq!(err.http_status.as_u16(), 500);
    assert!(err.message.contains("stripe"));
    assert!(err.cause.is_some());
    assert_eq!(
        err.details.get("service").and_then(|v| v.as_str()),
        Some("stripe")
    );
}

#[test]
fn wrap_delegates_to_internal() {
    let cause = std::io::Error::other("oops");
    let err = AppError::wrap(cause);
    assert_eq!(err.code, ErrorCode::Internal);
    assert!(err.cause.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Builder pattern
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn with_cause_preserves_cause() {
    let cause = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let err = AppError::new(ErrorCode::Timeout, "timeout").with_cause(cause);
    assert!(err.cause.is_some());
    let source = err.source().unwrap();
    assert!(source.to_string().contains("timed out"));
}

#[test]
fn with_detail_adds_entry() {
    let err = AppError::new(ErrorCode::InvalidInput, "bad").with_detail("field", "name");
    assert_eq!(
        err.details.get("field").and_then(|v| v.as_str()),
        Some("name")
    );
}

#[test]
fn with_details_merges_map() {
    let mut details = HashMap::new();
    details.insert("a".to_string(), serde_json::json!("one"));
    details.insert("b".to_string(), serde_json::json!(2));

    let err = AppError::new(ErrorCode::Internal, "err").with_details(details);
    assert_eq!(err.details.len(), 2);
    assert_eq!(err.details.get("a").and_then(|v| v.as_str()), Some("one"));
    assert_eq!(err.details.get("b").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn with_details_merges_with_existing() {
    let mut extra = HashMap::new();
    extra.insert("y".to_string(), serde_json::json!("val"));

    let err = AppError::new(ErrorCode::Internal, "err")
        .with_detail("x", "existing")
        .with_details(extra);
    assert_eq!(err.details.len(), 2);
    assert!(err.details.contains_key("x"));
    assert!(err.details.contains_key("y"));
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

    assert_eq!(err.code, ErrorCode::ExternalService);
    assert!(!err.is_retryable());
    assert!(err.cause.is_some());
    assert_eq!(err.details.len(), 2);
    assert_eq!(
        err.details.get("service").and_then(|v| v.as_str()),
        Some("api-x")
    );
    assert_eq!(
        err.details.get("endpoint").and_then(|v| v.as_str()),
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
        (ErrorCode::Internal, "oops", "INTERNAL: oops"),
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
fn from_io_error_maps_to_internal() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
    let err: AppError = io_err.into();
    assert_eq!(err.code, ErrorCode::Internal);
    assert!(err.message.contains("no access"));
}

#[test]
fn from_serde_json_error_maps_to_invalid_format() {
    let json_err = serde_json::from_str::<serde_json::Value>("not json {{{").unwrap_err();
    let err: AppError = json_err.into();
    assert_eq!(err.code, ErrorCode::InvalidFormat);
    assert!(!err.message.is_empty());
}

#[test]
fn from_fmt_error_maps_to_internal() {
    let fmt_err = std::fmt::Error;
    let err: AppError = fmt_err.into();
    assert_eq!(err.code, ErrorCode::Internal);
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
    assert_eq!(result.unwrap_err().code, ErrorCode::NotFound);
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
    assert_eq!(result.unwrap_err().code, ErrorCode::Internal);
}

#[test]
fn app_result_map_err() {
    let result: Result<i32, String> = Err("bad input".to_string());
    let app_result: AppResult<i32> = result.map_err(|msg| AppError::invalid_input("field", msg));
    assert!(app_result.is_err());
    let err = app_result.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("bad input"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. ErrorResponse — serialization, deserialization, structure
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn error_response_from_app_error_fields() {
    let err = AppError::not_found("User", Some("42"));
    let resp = ErrorResponse::from(&err);
    assert_eq!(resp.status, 404);
    assert_eq!(resp.title, "NOT_FOUND");
    assert!(resp.detail.contains("User"));
    assert!(resp.detail.contains("42"));
    assert!(resp.error_type.contains("not-found"));
    assert!(resp.instance.is_none());
    assert!(resp.extensions.is_empty());
}

#[test]
fn error_response_error_type_uri_format() {
    let cases: Vec<(ErrorCode, &str)> = vec![
        (ErrorCode::ServiceUnavailable, "service-unavailable"),
        (ErrorCode::ConnectionFailed, "connection-failed"),
        (ErrorCode::TokenExpired, "token-expired"),
        (ErrorCode::InvalidInput, "invalid-input"),
        (ErrorCode::DatabaseError, "database-error"),
        (ErrorCode::ExternalService, "external-service"),
    ];
    for (code, expected_slug) in cases {
        let err = AppError::new(code, "test");
        let resp = ErrorResponse::from(&err);
        let expected_uri = format!("https://rskit.dev/errors/{}", expected_slug);
        assert_eq!(resp.error_type, expected_uri, "URI for {:?}", code);
    }
}

#[test]
fn error_response_from_owned_app_error() {
    let err = AppError::unauthorized("no token");
    let resp = ErrorResponse::from(err);
    assert_eq!(resp.status, 401);
    assert_eq!(resp.detail, "no token");
}

#[test]
fn error_response_serializes_to_json() {
    let err = AppError::new(ErrorCode::NotFound, "item not found");
    let resp = ErrorResponse::from(&err);
    let json = serde_json::to_value(&resp).unwrap();

    assert_eq!(json["type"], "https://rskit.dev/errors/not-found");
    assert_eq!(json["title"], "NOT_FOUND");
    assert_eq!(json["status"], 404);
    assert_eq!(json["detail"], "item not found");
    // instance is None → should be absent due to skip_serializing_if
    assert!(json.get("instance").is_none());
    // extensions is empty → should be absent due to skip_serializing_if
    assert!(json.get("extensions").is_none());
}

#[test]
fn error_response_json_roundtrip() {
    let err = AppError::new(ErrorCode::Forbidden, "not allowed");
    let resp = ErrorResponse::from(&err);
    let json_str = serde_json::to_string(&resp).unwrap();
    let deserialized: ErrorResponse = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.status, resp.status);
    assert_eq!(deserialized.title, resp.title);
    assert_eq!(deserialized.detail, resp.detail);
    assert_eq!(deserialized.error_type, resp.error_type);
}

#[test]
fn error_response_with_extensions_roundtrip() {
    let mut resp = ErrorResponse::from(&AppError::new(ErrorCode::Internal, "err"));
    resp.extensions
        .insert("trace_id".to_string(), "abc-123".to_string());
    resp.instance = Some("/api/v1/users/42".to_string());

    let json_str = serde_json::to_string(&resp).unwrap();
    let deserialized: ErrorResponse = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.extensions.get("trace_id").unwrap(), "abc-123");
    assert_eq!(deserialized.instance.as_deref(), Some("/api/v1/users/42"));
}

#[test]
fn error_response_deserialize_from_raw_json() {
    let raw = r#"{
        "type": "https://rskit.dev/errors/not-found",
        "title": "NOT_FOUND",
        "status": 404,
        "detail": "user not found"
    }"#;
    let resp: ErrorResponse = serde_json::from_str(raw).unwrap();
    assert_eq!(resp.status, 404);
    assert_eq!(resp.title, "NOT_FOUND");
    assert_eq!(resp.detail, "user not found");
    assert!(resp.extensions.is_empty());
    assert!(resp.instance.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_string_message() {
    let err = AppError::new(ErrorCode::Internal, "");
    assert_eq!(err.message, "");
    assert_eq!(format!("{err}"), "INTERNAL: ");
}

#[test]
fn very_long_message() {
    let long_msg = "x".repeat(10_000);
    let err = AppError::new(ErrorCode::Internal, long_msg.clone());
    assert_eq!(err.message, long_msg);
    assert_eq!(err.message.len(), 10_000);
}

#[test]
fn unicode_in_message() {
    let err = AppError::new(ErrorCode::InvalidInput, "名前が無効です 🚀");
    assert_eq!(err.message, "名前が無効です 🚀");
    assert!(format!("{err}").contains("🚀"));
}

#[test]
fn unicode_in_details() {
    let err = AppError::new(ErrorCode::InvalidInput, "err")
        .with_detail("field", "名前")
        .with_detail("emoji", "🎉");
    assert_eq!(
        err.details.get("field").and_then(|v| v.as_str()),
        Some("名前")
    );
    assert_eq!(
        err.details.get("emoji").and_then(|v| v.as_str()),
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

    assert_eq!(err.details.len(), 5);
    assert!(err.details["array"].is_array());
    assert!(err.details["nested"].is_object());
    assert!(err.details["null_val"].is_null());
    assert_eq!(err.details["bool_val"].as_bool(), Some(true));
    assert_eq!(err.details["number"].as_f64(), Some(42.5));
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
        (ErrorCode::Internal, "INTERNAL"),
        (ErrorCode::DatabaseError, "DATABASE_ERROR"),
        (ErrorCode::ExternalService, "EXTERNAL_SERVICE"),
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
        (ErrorCode::Internal, "INTERNAL"),
        (ErrorCode::DatabaseError, "DATABASE_ERROR"),
        (ErrorCode::ExternalService, "EXTERNAL_SERVICE"),
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

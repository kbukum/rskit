use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::AppError;
use crate::code::ErrorCode;

// The default base URI for RFC 9457 `type` fields produced by the `From` impls.
// Transport layers that need a custom domain call `ProblemDetail::with_base_uri`.
const DEFAULT_TYPE_BASE_URI: &str = "https://rskit.dev/errors/";

// ── ProblemDetail ─────────────────────────────────────────────────────────────

/// RFC 9457 Problem Details — the single canonical error response envelope for
/// all HTTP and gRPC error responses in rskit.
///
/// Serialises to:
/// ```json
/// {
///   "type": "https://rskit.dev/errors/not-found",
///   "title": "Not Found",
///   "status": 404,
///   "detail": "user '42' not found",
///   "code": "NOT_FOUND",
///   "retryable": false
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetail {
    /// URI reference that identifies the problem type (RFC 9457 §3.1.1).
    #[serde(rename = "type")]
    pub error_type: String,
    /// Short, human-readable summary of the problem type (RFC 9457 §3.1.2).
    pub title: String,
    /// HTTP status code (RFC 9457 §3.1.3).
    pub status: u16,
    /// Human-readable explanation specific to this occurrence (RFC 9457 §3.1.4).
    pub detail: String,
    /// URI reference identifying this specific occurrence (RFC 9457 §3.1.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Machine-readable error code (rskit extension).
    pub code: ErrorCode,
    /// Whether the operation that produced this error is safe to retry (rskit extension).
    pub retryable: bool,
    /// Arbitrary key-value pairs providing additional context (rskit extension).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub details: HashMap<String, serde_json::Value>,
}

/// Convert an `ErrorCode`'s `SCREAMING_SNAKE_CASE` representation to `kebab-case`.
fn code_to_kebab(code: ErrorCode) -> String {
    code.as_str().to_ascii_lowercase().replace('_', "-")
}

/// Convert an `ErrorCode`'s `SCREAMING_SNAKE_CASE` representation to `Title Case`.
fn code_to_title(code: ErrorCode) -> String {
    code.as_str()
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let rest: String = chars.map(|c| c.to_ascii_lowercase()).collect();
                    format!("{}{}", first.to_ascii_uppercase(), rest)
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl ProblemDetail {
    /// Build a [`ProblemDetail`] using `base_uri` as the RFC 9457 `type` prefix.
    ///
    /// The default [`From`] impls use `"https://rskit.dev/errors/"`. Call this
    /// constructor when the serving application should advertise its own error
    /// documentation domain (typically configured in the HTTP middleware layer).
    ///
    /// `base_uri` must end with `/`; a trailing slash is appended automatically
    /// if absent.
    pub fn with_base_uri(base_uri: &str, err: &AppError) -> Self {
        let base = if base_uri.ends_with('/') {
            base_uri.to_string()
        } else {
            format!("{base_uri}/")
        };
        Self::build(&base, err)
    }

    fn build(base_uri: &str, err: &AppError) -> Self {
        Self {
            error_type: format!("{}{}", base_uri, code_to_kebab(err.code)),
            title: code_to_title(err.code),
            status: err.code.http_status().as_u16(),
            detail: err.message.clone(),
            instance: None,
            code: err.code,
            retryable: err.retryable,
            details: err.details.clone(),
        }
    }
}

impl From<&AppError> for ProblemDetail {
    /// Converts using the default `https://rskit.dev/errors/` type URI base.
    ///
    /// Suitable for internal tooling, tests, and gRPC detail payloads.
    /// **Production HTTP services** should call [`ProblemDetail::with_base_uri`]
    /// so the `type` URI advertises the application's own error-documentation
    /// domain rather than `rskit.dev`.
    fn from(err: &AppError) -> Self {
        Self::build(DEFAULT_TYPE_BASE_URI, err)
    }
}

impl From<AppError> for ProblemDetail {
    /// Converts using the default `https://rskit.dev/errors/` type URI base.
    ///
    /// See [`From<&AppError>`] for guidance on when to use
    /// [`ProblemDetail::with_base_uri`] instead.
    fn from(err: AppError) -> Self {
        ProblemDetail::from(&err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppError, ErrorCode};

    // ── code_to_kebab ─────────────────────────────────────────────────────

    #[test]
    fn kebab_not_found() {
        assert_eq!(code_to_kebab(ErrorCode::NotFound), "not-found");
    }

    #[test]
    fn kebab_service_unavailable() {
        assert_eq!(
            code_to_kebab(ErrorCode::ServiceUnavailable),
            "service-unavailable"
        );
    }

    #[test]
    fn kebab_internal() {
        assert_eq!(code_to_kebab(ErrorCode::Internal), "internal-error");
    }

    // ── code_to_title ─────────────────────────────────────────────────────

    #[test]
    fn title_not_found() {
        assert_eq!(code_to_title(ErrorCode::NotFound), "Not Found");
    }

    #[test]
    fn title_service_unavailable() {
        assert_eq!(
            code_to_title(ErrorCode::ServiceUnavailable),
            "Service Unavailable"
        );
    }

    #[test]
    fn title_internal() {
        assert_eq!(code_to_title(ErrorCode::Internal), "Internal Error");
    }

    // ── ProblemDetail::from(&AppError) ────────────────────────────────────

    #[test]
    fn from_app_error_sets_all_fields() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        let pd = ProblemDetail::from(&err);
        assert_eq!(pd.status, 404);
        assert_eq!(pd.detail, "item not found");
        assert_eq!(pd.title, "Not Found");
        assert_eq!(pd.code, ErrorCode::NotFound);
        assert!(!pd.retryable);
        assert!(pd.instance.is_none());
        assert!(pd.details.is_empty());
    }

    #[test]
    fn from_app_error_type_uri_uses_base_and_kebab() {
        let err = AppError::new(ErrorCode::NotFound, "gone");
        let pd = ProblemDetail::from(&err);
        assert!(
            pd.error_type.ends_with("not-found"),
            "type was: {}",
            pd.error_type
        );
        assert!(
            pd.error_type.starts_with("https://"),
            "type was: {}",
            pd.error_type
        );
    }

    #[test]
    fn from_app_error_retryable_flag() {
        let err = AppError::new(ErrorCode::ServiceUnavailable, "down");
        let pd = ProblemDetail::from(&err);
        assert!(pd.retryable);
    }

    #[test]
    fn from_app_error_preserves_details() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad field").with_detail("field", "email");
        let pd = ProblemDetail::from(&err);
        assert_eq!(
            pd.details.get("field").and_then(|v| v.as_str()),
            Some("email")
        );
    }

    #[test]
    fn from_owned_app_error() {
        let err = AppError::new(ErrorCode::Unauthorized, "bad token");
        let pd = ProblemDetail::from(err);
        assert_eq!(pd.status, 401);
        assert_eq!(pd.code, ErrorCode::Unauthorized);
    }

    // ── Serialisation ─────────────────────────────────────────────────────

    #[test]
    fn serializes_to_rfc9457_shape() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        let pd = ProblemDetail::from(&err);
        let json = serde_json::to_value(&pd).unwrap();

        assert!(json.get("type").is_some());
        assert_eq!(json["title"], "Not Found");
        assert_eq!(json["status"], 404);
        assert_eq!(json["detail"], "item not found");
        assert_eq!(json["code"], "NOT_FOUND");
        assert_eq!(json["retryable"], false);
        assert!(json.get("instance").is_none(), "instance should be absent");
        assert!(
            json.get("details").is_none(),
            "empty details should be absent"
        );
    }

    #[test]
    fn serializes_instance_when_set() {
        let mut pd = ProblemDetail::from(&AppError::new(ErrorCode::NotFound, "gone"));
        pd.instance = Some("/api/users/42".into());
        let json = serde_json::to_value(&pd).unwrap();
        assert_eq!(json["instance"], "/api/users/42");
    }

    #[test]
    fn json_roundtrip() {
        let err = AppError::new(ErrorCode::Forbidden, "not allowed")
            .with_detail("resource", "admin_panel");
        let pd = ProblemDetail::from(&err);
        let json_str = serde_json::to_string(&pd).unwrap();
        let back: ProblemDetail = serde_json::from_str(&json_str).unwrap();

        assert_eq!(back.code, pd.code);
        assert_eq!(back.detail, pd.detail);
        assert_eq!(back.retryable, pd.retryable);
        assert_eq!(back.details, pd.details);
        assert_eq!(back.error_type, pd.error_type);
        assert_eq!(back.title, pd.title);
        assert_eq!(back.status, pd.status);
    }

    #[test]
    fn json_roundtrip_with_instance_and_details() {
        let mut pd = ProblemDetail::from(&AppError::new(ErrorCode::Internal, "err"));
        pd.instance = Some("/api/v1/users/42".into());
        pd.details
            .insert("trace_id".into(), serde_json::json!("abc-123"));

        let json_str = serde_json::to_string(&pd).unwrap();
        let back: ProblemDetail = serde_json::from_str(&json_str).unwrap();

        assert_eq!(
            back.details.get("trace_id").and_then(|v| v.as_str()),
            Some("abc-123")
        );
        assert_eq!(back.instance.as_deref(), Some("/api/v1/users/42"));
    }

    #[test]
    fn deserialize_from_raw_json() {
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

    // ── ProblemDetail::with_base_uri ───────────────────────────────────────

    #[test]
    fn with_base_uri_overrides_type_prefix() {
        let err = AppError::new(ErrorCode::NotFound, "gone");
        let pd = ProblemDetail::with_base_uri("https://example.com/errors", &err);
        assert!(
            pd.error_type.starts_with("https://example.com/errors/"),
            "type was: {}",
            pd.error_type
        );
        assert!(
            pd.error_type.ends_with("not-found"),
            "type was: {}",
            pd.error_type
        );
    }

    #[test]
    fn with_base_uri_appends_trailing_slash() {
        let err = AppError::new(ErrorCode::NotFound, "gone");
        let without = ProblemDetail::with_base_uri("https://example.com/errors", &err);
        let with_slash = ProblemDetail::with_base_uri("https://example.com/errors/", &err);
        assert_eq!(without.error_type, with_slash.error_type);
    }

    #[test]
    fn default_from_uses_rskit_dev_base() {
        let err = AppError::new(ErrorCode::NotFound, "gone");
        let pd = ProblemDetail::from(&err);
        assert!(
            pd.error_type.starts_with("https://rskit.dev/errors/"),
            "type was: {}",
            pd.error_type
        );
    }

    #[test]
    fn error_type_uri_all_codes() {
        let cases: &[(ErrorCode, &str)] = &[
            (ErrorCode::ServiceUnavailable, "service-unavailable"),
            (ErrorCode::ConnectionFailed, "connection-failed"),
            (ErrorCode::TokenExpired, "token-expired"),
            (ErrorCode::InvalidInput, "invalid-input"),
            (ErrorCode::DatabaseError, "database-error"),
            (ErrorCode::ExternalService, "external-service-error"),
        ];
        for (code, expected_slug) in cases {
            let err = AppError::new(*code, "test");
            let pd = ProblemDetail::from(&err);
            assert!(
                pd.error_type.ends_with(expected_slug),
                "URI for {:?} was: {}",
                code,
                pd.error_type
            );
        }
    }
}

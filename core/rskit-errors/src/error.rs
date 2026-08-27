use std::collections::HashMap;

use serde_json::Value;

use crate::code::ErrorCode;

/// Application-level structured error.
///
/// Carries a machine-readable [`ErrorCode`], a human-readable message,
/// optional key→value details for rich error responses,
/// and an optional cause chain compatible with `std::error::Error`.
///
/// Fields are private to preserve the error's invariants. `http_status` is fully determined by `code`
/// and can never drift from it. `retryable` is seeded from `code`'s default
/// but may be intentionally overridden via the [`AppError::retryable`] builder;
/// read access is via the getter methods
/// and mutation via the builder methods (`with_*`, `retryable`, `context`).
///
/// `details` deliberately uses [`serde_json::Value`]:
/// it models RFC 9457 problem-detail *extension members*, which are by definition arbitrary JSON
/// and cannot be given a closed type without losing that openness.
#[derive(Debug)]
pub struct AppError {
    /// Machine-readable error classification.
    code: ErrorCode,
    /// Human-readable description of the error.
    message: String,
    /// Whether the operation that produced this error is safe to retry.
    retryable: bool,
    /// Canonical HTTP status code for this error.
    http_status: http::StatusCode,
    /// Arbitrary key-value pairs for rich error responses.
    details: HashMap<String, Value>,
    /// Optional underlying error that caused this one.
    cause: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_deref().map(|e| e as _)
    }
}

impl AppError {
    // ── Core constructor ────────────────────────────────────────────────

    /// Create a new [`AppError`] from `code` and a human-readable `message`.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = code.is_retryable();
        let http_status = code.http_status();
        Self {
            code,
            message: message.into(),
            retryable,
            http_status,
            details: HashMap::new(),
            cause: None,
        }
    }

    // ── Fluent builder methods ──────────────────────────────────────────

    /// Attach an underlying cause to this error.
    #[must_use]
    pub fn with_cause(mut self, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// Attach an already-boxed, type-erased error as the underlying cause.
    ///
    /// Use this when the source is a trait object (`Box<dyn Error + Send +
    /// Sync>`, as produced by crates like `tokenizers` or `anyhow`) that does
    /// not itself implement [`std::error::Error`] and so cannot be passed to
    /// [`with_cause`](Self::with_cause).
    #[must_use]
    pub fn with_boxed_cause(
        mut self,
        cause: Box<dyn std::error::Error + Send + Sync + 'static>,
    ) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Add a single key-value detail entry.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Merge a map of detail entries into this error.
    #[must_use]
    pub fn with_details(mut self, details: HashMap<String, Value>) -> Self {
        self.details.extend(details);
        self
    }

    /// Override whether this error is considered retryable.
    #[must_use]
    pub fn retryable(mut self, r: bool) -> Self {
        self.retryable = r;
        self
    }

    // ── Convenience constructors (mirror gokit's API surface) ───────────

    /// Create a `ServiceUnavailable` error for the named service.
    pub fn service_unavailable(service: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::ServiceUnavailable,
            format!("service unavailable: {}", service.into()),
        )
    }

    /// Create a `ConnectionFailed` error for the named service.
    pub fn connection_failed(service: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::ConnectionFailed,
            format!("connection failed: {}", service.into()),
        )
    }

    /// Create a `Timeout` error for the named operation.
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::Timeout,
            format!("operation timed out: {}", operation.into()),
        )
    }

    /// Create a `RateLimited` error.
    pub fn rate_limited() -> Self {
        Self::new(ErrorCode::RateLimited, "rate limit exceeded")
    }

    /// Create a `NotFound` error for `resource`, optionally including an `id`.
    pub fn not_found(resource: impl Into<String>, id: Option<&str>) -> Self {
        let msg = match id {
            Some(id) => format!("{} '{}' not found", resource.into(), id),
            None => format!("{} not found", resource.into()),
        };
        Self::new(ErrorCode::NotFound, msg)
    }

    /// Create an `AlreadyExists` error for the named resource.
    pub fn already_exists(resource: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::AlreadyExists,
            format!("{} already exists", resource.into()),
        )
    }

    /// Create a `Conflict` error with the given reason.
    pub fn conflict(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, reason)
    }

    /// Create an `InvalidInput` error for the named field.
    pub fn invalid_input(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidInput,
            format!("invalid {}: {}", field.into(), reason.into()),
        )
    }

    /// Create a `MissingField` error for the named required field.
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::MissingField,
            format!("missing required field: {}", field.into()),
        )
    }

    /// Create an `InvalidFormat` error for the named field.
    pub fn invalid_format(field: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidFormat,
            format!(
                "invalid format for {}: expected {}",
                field.into(),
                expected.into()
            ),
        )
    }

    /// Create an `Unauthorized` error with the given reason.
    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, reason)
    }

    /// Create a `Forbidden` error with the given reason.
    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, reason)
    }

    /// Create a `TokenExpired` error.
    pub fn token_expired() -> Self {
        Self::new(ErrorCode::TokenExpired, "authentication token has expired")
    }

    /// Create an `InvalidToken` error.
    pub fn invalid_token() -> Self {
        Self::new(ErrorCode::InvalidToken, "authentication token is invalid")
    }

    /// Wrap an arbitrary error as an `Internal` application error.
    ///
    /// The cause is stored internally for logging but is NOT included in the serialized response —
    /// callers receive a generic "internal server error" message.
    pub fn internal(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ErrorCode::Internal, "internal server error").with_cause(cause)
    }

    /// Wrap a database error.
    ///
    /// The cause is stored internally for logging but is NOT included in the serialized response —
    /// callers receive a generic "database error" message.
    pub fn database_error(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::new(ErrorCode::DatabaseError, "database error").with_cause(cause)
    }

    /// Wrap an external service error, naming the dependency.
    ///
    /// The cause is stored internally for logging but is NOT included in the serialized response —
    /// callers receive a generic message naming only the service.
    pub fn external_service(
        service: impl Into<String>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let svc = service.into();
        let msg = format!("external service error ({svc})");
        Self::new(ErrorCode::ExternalService, msg)
            .with_cause(cause)
            .with_detail("service", svc)
    }

    /// Create a `Cancelled` error for the named operation.
    pub fn cancelled(operation: impl Into<String>) -> Self {
        let op = operation.into();
        Self::new(
            ErrorCode::Cancelled,
            format!("operation '{}' was cancelled", op),
        )
        .with_detail("operation", op)
    }

    /// Add human-readable context to this error.
    ///
    /// Prepends `msg` to the existing error message so call-site context is preserved in logs
    /// and error responses without losing the original cause.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use rskit_errors::{AppError, ErrorCode};
    /// let err = AppError::new(ErrorCode::NotFound, "user not found")
    ///     .context("load profile");
    /// assert_eq!(err.message(), "load profile: user not found");
    /// ```
    #[must_use]
    pub fn context(mut self, msg: impl Into<String>) -> Self {
        let new_msg = format!("{}: {}", msg.into(), self.message);
        self.message = new_msg;
        self
    }

    /// Append a trailing hint after the existing error message.
    ///
    /// The counterpart to [`context`](Self::context): where `context` prepends call-site context,
    /// `hint` appends advisory guidance (for example a "did you mean …?" suggestion) as a new sentence,
    /// preserving the code, cause, structured details, and retryability of the original error.
    ///
    /// An empty or whitespace-only hint is a no-op, so an optional
    /// or derived suggestion can be threaded through without a conditional at the call site.
    /// The hint is trimmed of surrounding whitespace
    /// and appended onto the existing message (separated by a single space when the message is non-empty);
    /// the caller supplies any punctuation within the `hint` itself.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use rskit_errors::{AppError, ErrorCode};
    /// let err = AppError::invalid_input("task", "no such task 'buld'")
    ///     .hint("Did you mean 'build'?");
    /// assert_eq!(
    ///     err.message(),
    ///     "invalid task: no such task 'buld' Did you mean 'build'?"
    /// );
    /// ```
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        let hint = hint.into();
        let hint = hint.trim();
        if hint.is_empty() {
            return self;
        }
        if !self.message.is_empty() {
            self.message.push(' ');
        }
        self.message.push_str(hint);
        self
    }

    // ── Query helpers ───────────────────────────────────────────────────

    /// Returns `true` if the operation that produced this error is safe to retry.
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    /// Returns `true` if this error indicates a missing resource.
    pub fn is_not_found(&self) -> bool {
        self.code == ErrorCode::NotFound
    }

    /// Returns `true` if this error is an authentication/authorisation failure.
    pub fn is_unauthorized(&self) -> bool {
        matches!(
            self.code,
            ErrorCode::Unauthorized | ErrorCode::TokenExpired | ErrorCode::InvalidToken
        )
    }

    /// Machine-readable error classification.
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Canonical HTTP status code for this error.
    pub const fn http_status(&self) -> http::StatusCode {
        self.http_status
    }

    /// Additional structured error details.
    pub fn details(&self) -> &HashMap<String, Value> {
        &self.details
    }

    /// Underlying source error, if one was attached.
    pub fn cause(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.cause.as_deref()
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = ser.serialize_struct("AppError", 4)?;
        s.serialize_field("code", &self.code)?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("retryable", &self.retryable)?;
        if !self.details.is_empty() {
            s.serialize_field("details", &self.details)?;
        }
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AppError::new ─────────────────────────────────────────────────────────

    #[test]
    fn new_sets_code_and_message() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        assert_eq!(err.code, ErrorCode::NotFound);
        assert_eq!(err.message, "item not found");
    }

    #[test]
    fn new_sets_retryable_true_for_retryable_code() {
        let err = AppError::new(ErrorCode::ConnectionFailed, "conn error");
        assert!(err.retryable);
    }

    #[test]
    fn new_sets_retryable_false_for_non_retryable_code() {
        let err = AppError::new(ErrorCode::Unauthorized, "no access");
        assert!(!err.retryable);
    }

    #[test]
    fn new_sets_http_status_from_code() {
        let err = AppError::new(ErrorCode::NotFound, "missing");
        assert_eq!(err.http_status, http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn new_starts_with_empty_details() {
        let err = AppError::new(ErrorCode::Internal, "oops");
        assert!(err.details.is_empty());
    }

    #[test]
    fn new_starts_with_no_cause() {
        let err = AppError::new(ErrorCode::Internal, "oops");
        assert!(err.cause.is_none());
    }

    // ── with_detail ───────────────────────────────────────────────────────────

    #[test]
    fn with_detail_stores_single_kv() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad field").with_detail("field", "email");
        assert_eq!(
            err.details.get("field").and_then(|v| v.as_str()),
            Some("email")
        );
    }

    #[test]
    fn with_detail_stores_multiple_kv() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad")
            .with_detail("field", "email")
            .with_detail("reason", "invalid format");
        assert_eq!(err.details.len(), 2);
        assert!(err.details.contains_key("field"));
        assert!(err.details.contains_key("reason"));
    }

    // ── with_cause ────────────────────────────────────────────────────────────

    #[test]
    fn with_cause_stores_cause() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let err = AppError::new(ErrorCode::Timeout, "timed out").with_cause(io_err);
        assert!(err.cause.is_some());
    }

    #[test]
    fn with_cause_source_returns_cause() {
        use std::error::Error;
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        let err = AppError::new(ErrorCode::ConnectionFailed, "conn failed").with_cause(io_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn with_boxed_cause_stores_type_erased_source() {
        use std::error::Error;
        use std::io;
        // A trait object that does not itself implement `Error`.
        let boxed: Box<dyn Error + Send + Sync> = Box::new(io::Error::other("erased"));
        let err = AppError::new(ErrorCode::Internal, "wrapped").with_boxed_cause(boxed);
        assert!(err.cause.is_some());
        assert_eq!(err.source().unwrap().to_string(), "erased");
    }

    // ── Convenience constructors ──────────────────────────────────────────────

    #[test]
    fn hint_appends_after_message_preserving_code() {
        let err =
            AppError::invalid_input("task", "no such task 'buld'").hint("Did you mean 'build'?");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert_eq!(
            err.message,
            "invalid task: no such task 'buld' Did you mean 'build'?"
        );
    }

    #[test]
    fn hint_preserves_cause_and_details() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err = AppError::new(ErrorCode::InvalidInput, "bad")
            .with_detail("field", "task")
            .with_cause(io_err)
            .hint("try again");
        assert_eq!(err.message, "bad try again");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert_eq!(err.retryable, ErrorCode::InvalidInput.is_retryable());
        assert_eq!(err.http_status, ErrorCode::InvalidInput.http_status());
        assert!(err.cause.is_some());
        assert_eq!(
            err.details.get("field").and_then(|v| v.as_str()),
            Some("task")
        );
    }

    #[test]
    fn hint_is_a_no_op_for_an_empty_hint() {
        let err = AppError::invalid_input("task", "no such task 'buld'").hint("");
        assert_eq!(err.message, "invalid task: no such task 'buld'");
    }

    #[test]
    fn hint_is_a_no_op_for_a_whitespace_only_hint() {
        let err = AppError::invalid_input("task", "no such task 'buld'").hint("   \t");
        assert_eq!(err.message, "invalid task: no such task 'buld'");
    }

    #[test]
    fn hint_trims_surrounding_whitespace_to_a_single_separator() {
        let err = AppError::new(ErrorCode::InvalidInput, "bad").hint("  try again  ");
        assert_eq!(err.message, "bad try again");
    }

    #[test]
    fn not_found_without_id() {
        let err = AppError::not_found("User", None);
        assert_eq!(err.code, ErrorCode::NotFound);
        assert!(err.message.contains("User"));
        assert!(!err.retryable);
    }

    #[test]
    fn not_found_with_id() {
        let err = AppError::not_found("User", Some("42"));
        assert_eq!(err.code, ErrorCode::NotFound);
        assert!(err.message.contains("42"));
    }

    #[test]
    fn unauthorized_sets_code_and_not_retryable() {
        let err = AppError::unauthorized("token missing");
        assert_eq!(err.code, ErrorCode::Unauthorized);
        assert!(!err.retryable);
    }

    #[test]
    fn invalid_input_sets_code() {
        let err = AppError::invalid_input("email", "must contain @");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.message.contains("email"));
        assert!(err.message.contains("must contain @"));
    }

    #[test]
    fn timeout_is_retryable() {
        let err = AppError::timeout("db query");
        assert_eq!(err.code, ErrorCode::Timeout);
        assert!(err.retryable);
        assert!(err.message.contains("db query"));
    }

    #[test]
    fn rate_limited_is_retryable() {
        let err = AppError::rate_limited();
        assert_eq!(err.code, ErrorCode::RateLimited);
        assert!(err.retryable);
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_includes_message() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        let display = format!("{err}");
        assert!(display.contains("item not found"), "display was: {display}");
    }

    #[test]
    fn display_includes_code() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        let display = format!("{err}");
        assert!(display.contains("NOT_FOUND"), "display was: {display}");
    }

    // ── Query helpers ─────────────────────────────────────────────────────────

    #[test]
    fn is_retryable_reflects_retryable_field() {
        let err = AppError::new(ErrorCode::ServiceUnavailable, "down");
        assert!(err.is_retryable());
    }

    #[test]
    fn is_not_found_true_for_not_found_code() {
        let err = AppError::not_found("Resource", None);
        assert!(err.is_not_found());
    }

    #[test]
    fn is_not_found_false_for_other_code() {
        let err = AppError::new(ErrorCode::Internal, "err");
        assert!(!err.is_not_found());
    }

    #[test]
    fn is_unauthorized_true_for_unauthorized_code() {
        let err = AppError::unauthorized("denied");
        assert!(err.is_unauthorized());
    }

    #[test]
    fn is_unauthorized_true_for_token_expired() {
        let err = AppError::token_expired();
        assert!(err.is_unauthorized());
    }
}

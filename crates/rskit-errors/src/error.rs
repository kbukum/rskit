use std::collections::HashMap;

use serde_json::Value;

use crate::code::ErrorCode;

/// Application-level structured error.
///
/// Carries a machine-readable [`ErrorCode`], a human-readable message,
/// optional key→value details for rich error responses, and an optional
/// cause chain compatible with `std::error::Error`.
#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub http_status: http::StatusCode,
    pub details: HashMap<String, Value>,
    pub cause: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
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

    pub fn with_cause(
        mut self,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    pub fn with_details(mut self, details: HashMap<String, Value>) -> Self {
        self.details.extend(details);
        self
    }

    pub fn retryable(mut self, r: bool) -> Self {
        self.retryable = r;
        self
    }

    // ── Convenience constructors (mirror gokit's API surface) ───────────

    pub fn service_unavailable(service: impl Into<String>) -> Self {
        Self::new(ErrorCode::ServiceUnavailable, format!("service unavailable: {}", service.into()))
    }

    pub fn connection_failed(service: impl Into<String>) -> Self {
        Self::new(ErrorCode::ConnectionFailed, format!("connection failed: {}", service.into()))
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, format!("operation timed out: {}", operation.into()))
    }

    pub fn rate_limited() -> Self {
        Self::new(ErrorCode::RateLimited, "rate limit exceeded")
    }

    pub fn not_found(resource: impl Into<String>, id: Option<&str>) -> Self {
        let msg = match id {
            Some(id) => format!("{} '{}' not found", resource.into(), id),
            None => format!("{} not found", resource.into()),
        };
        Self::new(ErrorCode::NotFound, msg)
    }

    pub fn already_exists(resource: impl Into<String>) -> Self {
        Self::new(ErrorCode::AlreadyExists, format!("{} already exists", resource.into()))
    }

    pub fn conflict(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, reason)
    }

    pub fn invalid_input(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, format!("invalid {}: {}", field.into(), reason.into()))
    }

    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::new(ErrorCode::MissingField, format!("missing required field: {}", field.into()))
    }

    pub fn invalid_format(field: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidFormat,
            format!("invalid format for {}: expected {}", field.into(), expected.into()),
        )
    }

    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, reason)
    }

    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, reason)
    }

    pub fn token_expired() -> Self {
        Self::new(ErrorCode::TokenExpired, "authentication token has expired")
    }

    pub fn invalid_token() -> Self {
        Self::new(ErrorCode::InvalidToken, "authentication token is invalid")
    }

    pub fn internal(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        let msg = cause.to_string();
        Self::new(ErrorCode::Internal, msg).with_cause(cause)
    }

    pub fn database_error(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        let msg = format!("database error: {}", cause);
        Self::new(ErrorCode::DatabaseError, msg).with_cause(cause)
    }

    pub fn external_service(
        service: impl Into<String>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let svc = service.into();
        let msg = format!("external service error ({}): {}", svc, cause);
        Self::new(ErrorCode::ExternalService, msg)
            .with_cause(cause)
            .with_detail("service", svc)
    }

    /// Wrap any error as an internal AppError.
    pub fn wrap(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::internal(err)
    }

    // ── Query helpers ───────────────────────────────────────────────────

    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    pub fn is_not_found(&self) -> bool {
        self.code == ErrorCode::NotFound
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(
            self.code,
            ErrorCode::Unauthorized | ErrorCode::TokenExpired | ErrorCode::InvalidToken
        )
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

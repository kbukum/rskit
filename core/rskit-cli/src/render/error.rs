//! Machine-readable output format, exit codes, and error rendering.

use rskit_errors::{AppError, ErrorCode};
use serde::Serialize;

/// Machine-readable output format for CLI renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Human-readable terminal text.
    #[default]
    Text,
    /// JSON object or array.
    Json,
    /// YAML document.
    Yaml,
}

/// Process exit code convention shared by rskit CLIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[non_exhaustive]
pub enum ExitCode {
    /// Successful command.
    Success = 0,
    /// Invalid command input or configuration.
    Usage = 2,
    /// Authentication or authorization failure.
    Permission = 3,
    /// Requested resource was not found.
    NotFound = 4,
    /// Conflict with current state.
    Conflict = 5,
    /// Remote dependency or service failure.
    Unavailable = 69,
    /// Command was rate limited.
    RateLimited = 75,
    /// Command timed out.
    Timeout = 124,
    /// Command was cancelled.
    Cancelled = 130,
    /// Unclassified failure.
    Failure = 1,
}

impl ExitCode {
    /// Return this exit code as an integer suitable for `std::process::exit`.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

impl From<ErrorCode> for ExitCode {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::InvalidInput | ErrorCode::InvalidFormat | ErrorCode::MissingField => {
                Self::Usage
            }
            ErrorCode::Unauthorized
            | ErrorCode::Forbidden
            | ErrorCode::TokenExpired
            | ErrorCode::InvalidToken => Self::Permission,
            ErrorCode::NotFound => Self::NotFound,
            ErrorCode::Conflict | ErrorCode::AlreadyExists => Self::Conflict,
            ErrorCode::ServiceUnavailable
            | ErrorCode::ConnectionFailed
            | ErrorCode::ExternalService => Self::Unavailable,
            ErrorCode::RateLimited => Self::RateLimited,
            ErrorCode::Timeout => Self::Timeout,
            ErrorCode::Cancelled => Self::Cancelled,
            _ => Self::Failure,
        }
    }
}

/// Renders [`AppError`] values consistently for command-line applications.
pub struct ErrorRenderer {
    format: OutputFormat,
}

impl ErrorRenderer {
    /// Create a renderer for the requested output format.
    #[must_use]
    pub const fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Render an error and return the matching CLI exit code.
    #[must_use]
    pub fn render(&self, error: &AppError) -> (String, ExitCode) {
        let exit_code = ExitCode::from(error.code());
        let rendered = match self.format {
            OutputFormat::Text => format!("error[{}]: {}", error.code(), error.message()),
            OutputFormat::Json => serde_json::to_string(&ErrorEnvelope::new(error, exit_code))
                .unwrap_or_else(|_| fallback_json(error, exit_code)),
            OutputFormat::Yaml => serde_norway::to_string(&ErrorEnvelope::new(error, exit_code))
                .unwrap_or_else(|_| fallback_yaml(error, exit_code)),
        };
        (rendered, exit_code)
    }
}

impl Default for ErrorRenderer {
    fn default() -> Self {
        Self::new(OutputFormat::Text)
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    code: ErrorCode,
    message: &'a str,
    retryable: bool,
    http_status: u16,
    exit_code: i32,
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    details: serde_json::Map<String, serde_json::Value>,
}

impl<'a> ErrorEnvelope<'a> {
    fn new(error: &'a AppError, exit_code: ExitCode) -> Self {
        Self {
            code: error.code(),
            message: error.message(),
            retryable: error.is_retryable(),
            http_status: error.http_status().as_u16(),
            exit_code: exit_code.as_i32(),
            details: error.details().clone().into_iter().collect(),
        }
    }
}

fn fallback_json(error: &AppError, exit_code: ExitCode) -> String {
    format!(
        r#"{{"code":"{}","message":{},"exit_code":{}}}"#,
        error.code(),
        serde_json::Value::String(error.message().to_string()),
        exit_code.as_i32()
    )
}

fn fallback_yaml(error: &AppError, exit_code: ExitCode) -> String {
    format!(
        "code: {}\nmessage: {}\nexit_code: {}\n",
        error.code(),
        serde_json::Value::String(error.message().to_string()),
        exit_code.as_i32()
    )
}

#[cfg(test)]
mod tests {
    use super::{ErrorRenderer, ExitCode, OutputFormat};
    use rskit_errors::AppError;

    #[test]
    fn error_renderer_uses_same_exit_code_across_formats() {
        let err = AppError::not_found("repo", Some("missing"));
        for format in [OutputFormat::Text, OutputFormat::Json, OutputFormat::Yaml] {
            let (rendered, code) = ErrorRenderer::new(format).render(&err);
            assert_eq!(code, ExitCode::NotFound);
            assert!(rendered.contains("not found"));
        }
    }

    #[test]
    fn json_fallback_stays_valid_and_escapes_the_message() {
        let err = AppError::new(rskit_errors::ErrorCode::Internal, "boom \"quoted\"");
        let rendered = super::fallback_json(&err, ExitCode::Failure);
        let payload: serde_json::Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(payload["code"], "INTERNAL_ERROR");
        assert_eq!(payload["message"], "boom \"quoted\"");
        assert_eq!(payload["exit_code"], 1);
    }

    #[test]
    fn yaml_fallback_carries_code_message_and_exit_code() {
        let err = AppError::new(rskit_errors::ErrorCode::Internal, "boom");
        let rendered = super::fallback_yaml(&err, ExitCode::Failure);
        assert!(rendered.contains("code: INTERNAL_ERROR"));
        assert!(rendered.contains("exit_code: 1"));
        let payload: serde_json::Value = serde_norway::from_str(&rendered).expect("valid yaml");
        assert_eq!(payload["message"], "boom");
    }
}

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::AppError;

/// RFC 7807 Problem Details response body.
///
/// Provides a machine-readable, standardised JSON envelope for HTTP error
/// responses.  Map an [`AppError`] to this type before writing the HTTP body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// URI reference identifying the problem type.
    #[serde(rename = "type")]
    pub error_type: String,
    /// Short human-readable summary.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Human-readable explanation of this specific occurrence.
    pub detail: String,
    /// URI reference identifying this specific occurrence (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Additional context key-value pairs.
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub extensions: HashMap<String, String>,
}

impl ErrorResponse {
    /// Build an [`ErrorResponse`] from an [`AppError`].
    pub fn from_app_error(err: &AppError) -> Self {
        let code_str = err.code.as_str().to_lowercase().replace('_', "-");
        Self {
            error_type: format!("https://rskit.dev/errors/{code_str}"),
            title: err.code.as_str().to_string(),
            status: err.http_status.as_u16(),
            detail: err.message.clone(),
            instance: None,
            extensions: HashMap::new(),
        }
    }
}

impl From<&AppError> for ErrorResponse {
    fn from(err: &AppError) -> Self {
        Self::from_app_error(err)
    }
}

impl From<AppError> for ErrorResponse {
    fn from(err: AppError) -> Self {
        Self::from_app_error(&err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppError, ErrorCode};

    #[test]
    fn from_app_error_sets_status_and_detail() {
        let err = AppError::new(ErrorCode::NotFound, "item not found");
        let resp = ErrorResponse::from(&err);
        assert_eq!(resp.status, 404);
        assert_eq!(resp.detail, "item not found");
        assert_eq!(resp.title, "NOT_FOUND");
    }

    #[test]
    fn from_app_error_owned_works() {
        let err = AppError::new(ErrorCode::Unauthorized, "bad token");
        let resp = ErrorResponse::from(err);
        assert_eq!(resp.status, 401);
    }

    #[test]
    fn error_type_is_uri_with_kebab_case() {
        let err = AppError::new(ErrorCode::ServiceUnavailable, "down");
        let resp = ErrorResponse::from(&err);
        assert!(resp.error_type.contains("service-unavailable"));
    }

    #[test]
    fn extensions_empty_by_default() {
        let err = AppError::new(ErrorCode::Internal, "oops");
        let resp = ErrorResponse::from(&err);
        assert!(resp.extensions.is_empty());
    }
}

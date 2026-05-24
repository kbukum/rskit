//! HTTP response wrapper.

use bytes::Bytes;
use http::StatusCode;
use rskit_errors::{AppError, ErrorCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;

/// Wrapped HTTP response with convenience methods.
pub struct Response {
    /// Response status code
    pub status: StatusCode,

    /// Response headers
    pub headers: HashMap<String, String>,

    /// Response body bytes
    body: Bytes,
}

impl Response {
    /// Creates a new response.
    pub(crate) fn new(status: StatusCode, headers: HashMap<String, String>, body: Bytes) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Gets the response status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Gets the numeric response status code.
    #[must_use]
    pub fn status_u16(&self) -> u16 {
        self.status.as_u16()
    }

    /// Checks if the response status is successful (2xx).
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Gets the response headers.
    #[must_use]
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// Gets a header value by name (case-insensitive).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&String> {
        let lower_name = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower_name)
            .map(|(_, v)| v)
    }

    /// Gets the response body as a byte slice.
    #[must_use]
    pub fn body_bytes(&self) -> &Bytes {
        &self.body
    }

    /// Consumes the response and returns the body as bytes.
    pub fn into_bytes(self) -> Bytes {
        self.body
    }

    /// Converts the response body to a string.
    pub fn text(self) -> rskit_errors::AppResult<String> {
        String::from_utf8(self.body.to_vec())
            .map_err(|e| AppError::new(ErrorCode::InvalidInput, format!("invalid utf8: {}", e)))
    }

    /// Converts the response body to a string, returning a diagnostic marker for non-UTF-8 bodies.
    #[must_use]
    pub fn text_or_diagnostic(self) -> String {
        String::from_utf8(self.body.to_vec()).unwrap_or_else(|_| "<non-utf8 body>".to_string())
    }

    /// Parses the response body as JSON.
    pub fn json<T: DeserializeOwned>(self) -> rskit_errors::AppResult<T> {
        serde_json::from_slice(&self.body).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to parse json response: {}", e),
            )
        })
    }

    /// Returns the body text after converting non-2xx responses into an [`AppError`].
    pub fn checked_text(self) -> rskit_errors::AppResult<String> {
        self.error_for_status()?.text()
    }

    /// Returns the parsed JSON body after converting non-2xx responses into an [`AppError`].
    pub fn checked_json<T: DeserializeOwned>(self) -> rskit_errors::AppResult<T> {
        self.error_for_status()?.json()
    }

    /// Returns an error if the status is not 2xx.
    pub fn error_for_status(self) -> rskit_errors::AppResult<Self> {
        if self.status.is_success() {
            Ok(self)
        } else {
            let status = self.status;
            let code = match status.as_u16() {
                400 => ErrorCode::InvalidInput,
                401 => ErrorCode::Unauthorized,
                403 => ErrorCode::Forbidden,
                404 => ErrorCode::NotFound,
                409 => ErrorCode::Conflict,
                429 => ErrorCode::RateLimited,
                500 | 502 | 503 | 504 => ErrorCode::Internal,
                _ => ErrorCode::ExternalService,
            };

            let body_str = self.text_or_diagnostic();

            Err(AppError::new(
                code,
                format!(
                    "http error: {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                ),
            )
            .with_detail("status", status.as_u16().to_string())
            .with_detail("body", body_str))
        }
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body_len", &self.body.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_status_check() {
        let resp = Response::new(StatusCode::OK, HashMap::new(), Bytes::from("ok"));
        assert!(resp.is_success());

        let resp = Response::new(
            StatusCode::NOT_FOUND,
            HashMap::new(),
            Bytes::from("not found"),
        );
        assert!(!resp.is_success());
    }

    #[test]
    fn test_response_header_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let resp = Response::new(StatusCode::OK, headers, Bytes::new());
        assert_eq!(
            resp.header("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_response_json() {
        let json_data = r#"{"key":"value"}"#;
        let resp = Response::new(StatusCode::OK, HashMap::new(), Bytes::from(json_data));

        let parsed: serde_json::Value = resp.json().unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn checked_text_rejects_error_status_with_body_detail() {
        let resp = Response::new(
            StatusCode::TOO_MANY_REQUESTS,
            HashMap::new(),
            Bytes::from("slow down"),
        );

        let error = resp.checked_text().unwrap_err();
        assert!(error.to_string().contains("http error: 429"));
        assert_eq!(
            error
                .details()
                .get("body")
                .and_then(serde_json::Value::as_str),
            Some("slow down")
        );
    }

    #[test]
    fn text_or_diagnostic_handles_non_utf8_body() {
        let resp = Response::new(
            StatusCode::BAD_GATEWAY,
            HashMap::new(),
            Bytes::from(vec![0xff]),
        );

        assert_eq!(resp.text_or_diagnostic(), "<non-utf8 body>");
    }
}

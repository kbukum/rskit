//! Authentication types for HTTP requests.

use base64::Engine;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::fmt;

/// Authentication method for HTTP requests.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Auth {
    /// Bearer token authentication: `Authorization: Bearer <token>`
    Bearer(String),
    /// HTTP Basic authentication: `Authorization: Basic <base64(username:password)>`
    Basic {
        /// Username for basic authentication
        username: String,
        /// Password for basic authentication
        password: String,
    },
    /// API key authentication: custom header with key value
    ApiKey {
        /// Header name for the API key
        name: String,
        /// API key value
        value: String,
    },
    /// No authentication.
    #[default]
    None,
}

impl Auth {
    /// Creates a Bearer token auth.
    pub fn bearer(token: impl Into<String>) -> Self {
        Auth::Bearer(token.into())
    }

    /// Creates a Basic auth.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Auth::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Creates an API key auth with custom header name.
    pub fn api_key(name: impl Into<String>, value: impl Into<String>) -> Self {
        Auth::ApiKey {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Returns the header name and value for this authentication method.
    ///
    /// # Errors
    /// Returns an error when the configured API-key header is invalid.
    pub fn header(&self) -> AppResult<Option<(String, String)>> {
        match self {
            Auth::Bearer(token) => Ok(Some((
                "authorization".to_string(),
                format!("Bearer {token}"),
            ))),
            Auth::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(&credentials);
                Ok(Some((
                    "authorization".to_string(),
                    format!("Basic {encoded}"),
                )))
            }
            Auth::ApiKey { name, value } => {
                if name.parse::<http::HeaderName>().is_err() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!("invalid API key header name '{name}'"),
                    ));
                }
                if value.parse::<http::HeaderValue>().is_err() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!("invalid API key header value for '{name}'"),
                    ));
                }
                Ok(Some((name.clone(), value.clone())))
            }
            Auth::None => Ok(None),
        }
    }
}

impl fmt::Display for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Auth::Bearer(_) => write!(f, "Bearer"),
            Auth::Basic { .. } => write!(f, "Basic"),
            Auth::ApiKey { name, .. } => write!(f, "ApiKey({})", name),
            Auth::None => write!(f, "None"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_rejects_invalid_header_value() {
        let auth = Auth::api_key("x-api-key", "bad\nvalue");
        assert!(auth.header().is_err());
    }
}

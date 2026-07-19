//! Authentication types for HTTP requests.
//!
//! Credential values are stored as [`SecretString`] so debug output redacts bearer tokens,
//! basic passwords, and API keys.
//! Header values are exposed only by [`Auth::header`] when applying a request.

use base64::Engine;
use http::header::AUTHORIZATION;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_security::{BASIC_AUTH_SCHEME, BEARER_AUTH_SCHEME, SecretString};
use std::fmt;

/// Authentication method for HTTP requests.
///
/// Secret-bearing variants redact their values in [`Debug`](std::fmt::Debug) output while preserving the plaintext for request header application.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Auth {
    /// Bearer token authentication: `Authorization: Bearer <token>`
    Bearer(SecretString),
    /// HTTP Basic authentication: `Authorization: Basic <base64(username:password)>`
    Basic {
        /// Username for basic authentication
        username: String,
        /// Password for basic authentication
        password: SecretString,
    },
    /// API key authentication: custom header with key value
    ApiKey {
        /// Header name for the API key
        name: String,
        /// API key value
        value: SecretString,
    },
    /// No authentication.
    #[default]
    None,
}

impl Auth {
    /// Creates a Bearer token auth from plaintext.
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::bearer_secret(SecretString::new(token))
    }

    /// Creates a Bearer token auth from a redacting secret.
    pub fn bearer_secret(token: SecretString) -> Self {
        Auth::Bearer(token)
    }

    /// Creates a Basic auth from a plaintext password.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::basic_secret(username, SecretString::new(password))
    }

    /// Creates a Basic auth from a redacting password secret.
    pub fn basic_secret(username: impl Into<String>, password: SecretString) -> Self {
        Auth::Basic {
            username: username.into(),
            password,
        }
    }

    /// Creates an API key auth with custom header name and plaintext value.
    pub fn api_key(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::api_key_secret(name, SecretString::new(value))
    }

    /// Creates an API key auth with a custom header name and redacting secret value.
    pub fn api_key_secret(name: impl Into<String>, value: SecretString) -> Self {
        Auth::ApiKey {
            name: name.into(),
            value,
        }
    }

    /// Returns the header name and value for this authentication method.
    ///
    /// # Errors
    /// Returns an error when the configured API-key header is invalid.
    pub fn header(&self) -> AppResult<Option<(String, String)>> {
        match self {
            Auth::Bearer(token) => Ok(Some((
                AUTHORIZATION.as_str().to_string(),
                format!("{BEARER_AUTH_SCHEME} {}", token.expose()),
            ))),
            Auth::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password.expose());
                let encoded = base64::engine::general_purpose::STANDARD.encode(&credentials);
                Ok(Some((
                    AUTHORIZATION.as_str().to_string(),
                    format!("{BASIC_AUTH_SCHEME} {encoded}"),
                )))
            }
            Auth::ApiKey { name, value } => {
                if name.parse::<http::HeaderName>().is_err() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!("invalid API key header name '{name}'"),
                    ));
                }
                if value.expose().parse::<http::HeaderValue>().is_err() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!("invalid API key header value for '{name}'"),
                    ));
                }
                Ok(Some((name.clone(), value.expose().to_string())))
            }
            Auth::None => Ok(None),
        }
    }
}

impl fmt::Display for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Auth::Bearer(_) => write!(f, "{BEARER_AUTH_SCHEME}"),
            Auth::Basic { .. } => write!(f, "{BASIC_AUTH_SCHEME}"),
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

    #[test]
    fn api_key_rejects_invalid_header_name() {
        let auth = Auth::api_key("bad header", "secret");

        let error = auth.header().expect_err("invalid header name");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("invalid API key header name"));
    }

    #[test]
    fn api_key_and_none_headers_are_explicit() {
        assert_eq!(
            Auth::api_key("x-api-key", "secret").header().unwrap(),
            Some(("x-api-key".to_string(), "secret".to_string()))
        );
        assert_eq!(Auth::None.header().unwrap(), None);
        assert_eq!(Auth::None.to_string(), "None");
    }

    #[test]
    fn debug_redacts_secret_values() {
        let cases = [
            format!("{:?}", Auth::bearer("secret-token")),
            format!("{:?}", Auth::basic("user", "secret-password")),
            format!("{:?}", Auth::api_key("x-api-key", "secret-key")),
        ];

        for formatted in cases {
            assert!(formatted.contains("SecretString(***)"));
            assert!(!formatted.contains("secret-token"));
            assert!(!formatted.contains("secret-password"));
            assert!(!formatted.contains("secret-key"));
        }
    }

    #[test]
    fn header_exposes_secret_only_for_request_application() {
        let auth = Auth::bearer_secret(SecretString::new("secret-token"));

        assert_eq!(
            auth.header().unwrap(),
            Some((
                AUTHORIZATION.as_str().to_string(),
                format!("{BEARER_AUTH_SCHEME} secret-token")
            ))
        );
    }
}

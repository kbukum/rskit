//! Authentication types for HTTP requests.

use base64::Engine;
use std::fmt;

/// Authentication method for HTTP requests.
#[derive(Debug, Clone, Default)]
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

    /// Applies authentication to a request header map.
    pub(crate) fn apply(&self, headers: &mut reqwest::header::HeaderMap) {
        match self {
            Auth::Bearer(token) => {
                if let Ok(value) = format!("Bearer {}", token).parse() {
                    headers.insert(reqwest::header::AUTHORIZATION, value);
                }
            }
            Auth::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(&credentials);
                if let Ok(value) = format!("Basic {}", encoded).parse() {
                    headers.insert(reqwest::header::AUTHORIZATION, value);
                }
            }
            Auth::ApiKey { name, value } => {
                if let Ok(header_name) = name.parse::<reqwest::header::HeaderName>() {
                    if let Ok(header_value) = value.parse::<reqwest::header::HeaderValue>() {
                        headers.insert(header_name, header_value);
                    }
                }
            }
            Auth::None => {}
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

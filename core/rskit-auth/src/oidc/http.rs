use std::fmt;

use async_trait::async_trait;
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use serde_json::Value;

use super::error::OidcError;

#[async_trait]
/// Minimal async HTTP client contract used by OIDC.
pub trait OidcHttpClient: Send + Sync {
    /// Fetch JSON from a URL, optionally using bearer authentication.
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError>;
}

/// Default OIDC HTTP client backed by the canonical `rskit-httpclient`.
pub struct ReqwestOidcHttpClient {
    client: HttpClient,
}

impl ReqwestOidcHttpClient {
    /// Create a default OIDC HTTP client using the canonical rskit HTTP client.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new() -> Result<Self, OidcError> {
        Self::with_config(HttpClientConfig::new())
    }

    /// Create an OIDC HTTP client with explicit HTTP configuration.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn with_config(config: HttpClientConfig) -> Result<Self, OidcError> {
        let client = HttpClient::new(sanitize_oidc_config(config))
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))?;
        Ok(Self { client })
    }
}

impl fmt::Debug for ReqwestOidcHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestOidcHttpClient")
            .field("client", &"<redacted>")
            .finish()
    }
}

fn sanitize_oidc_config(mut config: HttpClientConfig) -> HttpClientConfig {
    config.base_url = None;
    config.default_headers.clear();
    config.auth = None;
    config
}

#[async_trait]
impl OidcHttpClient for ReqwestOidcHttpClient {
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError> {
        let mut request = Request::get(url);
        if let Some(token) = bearer_token {
            request = request.auth(Auth::bearer(token));
        }
        let response = self
            .client
            .send(request)
            .await
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))?;
        response
            .checked_json()
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::header::AUTHORIZATION;
    use rskit_httpclient::{Auth, HttpClientConfig};
    use rskit_security::BEARER_AUTH_SCHEME;

    use super::ReqwestOidcHttpClient;

    #[test]
    fn oidc_http_client_constructs_with_default_config() {
        assert!(ReqwestOidcHttpClient::new().is_ok());
    }

    #[test]
    fn oidc_http_client_accepts_explicit_config() {
        let config = HttpClientConfig::new().with_timeout(Duration::from_secs(5));
        assert!(ReqwestOidcHttpClient::with_config(config).is_ok());
    }

    #[test]
    fn oidc_http_client_redacts_debug_output() {
        let config = HttpClientConfig::new().with_auth(Auth::bearer("secret-token"));
        let client = ReqwestOidcHttpClient::with_config(config).unwrap();

        let formatted = format!("{client:?}");

        assert!(formatted.contains("<redacted>"));
        assert!(!formatted.contains("secret-token"));
        assert!(!formatted.contains("HttpClientConfig"));
    }

    #[test]
    fn oidc_http_client_drops_cross_origin_defaults() {
        let config = HttpClientConfig::new()
            .with_base_url("https://internal.example")
            .with_header(
                AUTHORIZATION.as_str(),
                format!("{BEARER_AUTH_SCHEME} leaked"),
            )
            .with_header("x-api-key", "leaked")
            .with_auth(Auth::api_key("x-other-key", "leaked"));

        let client = ReqwestOidcHttpClient::with_config(config).unwrap();
        let config = client.client.config();

        assert!(config.base_url.is_none());
        assert!(config.default_headers.is_empty());
        assert!(config.auth.is_none());
    }
}

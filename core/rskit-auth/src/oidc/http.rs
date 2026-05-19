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
#[derive(Debug)]
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
        let client = HttpClient::new(config)
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))?;
        Ok(Self { client })
    }
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
        if !response.is_success() {
            return Err(OidcError::ProviderUnreachable(format!(
                "provider returned HTTP {}",
                response.status()
            )));
        }
        response
            .json()
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rskit_httpclient::HttpClientConfig;

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
}

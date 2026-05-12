use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use super::error::OidcError;

#[async_trait]
/// Minimal async HTTP client contract used by OIDC.
pub trait OidcHttpClient: Send + Sync {
    /// Fetch JSON from a URL, optionally using bearer authentication.
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError>;
}

/// Default reqwest-backed OIDC HTTP client.
#[derive(Debug, Clone)]
pub struct ReqwestOidcHttpClient {
    client: Client,
}

impl Default for ReqwestOidcHttpClient {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl OidcHttpClient for ReqwestOidcHttpClient {
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError> {
        let mut request = self.client.get(url);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(OidcError::ProviderUnreachable(format!(
                "provider returned HTTP {status}"
            )));
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))
    }
}

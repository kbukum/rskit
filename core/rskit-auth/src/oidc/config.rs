use std::time::Duration;

use jsonwebtoken::Algorithm;
use reqwest::Url;

use super::error::OidcError;

/// `OpenID` Connect client type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OidcClientType {
    /// Public/native/browser clients. PKCE is mandatory.
    #[default]
    Public,
    /// Confidential server-side clients. PKCE is still recommended.
    Confidential,
}

/// OIDC validation and authorization configuration.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Issuer URL (e.g. `https://accounts.google.com`).
    pub issuer: String,
    /// Client ID registered with the provider.
    pub client_id: String,
    /// Exact redirect URI registered with the provider.
    pub redirect_uri: String,
    /// OIDC client type.
    pub client_type: OidcClientType,
    /// Accepted audience claim values.
    pub audience: Vec<String>,
    /// Accepted ID-token signing algorithms.
    pub allowed_algorithms: Vec<Algorithm>,
    /// Clock-skew tolerance for `exp` and `nbf`.
    pub clock_skew: Duration,
}

impl OidcConfig {
    /// Create a new OIDC configuration.
    #[must_use]
    pub fn new(
        issuer: impl Into<String>,
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
        client_type: OidcClientType,
    ) -> Self {
        let client_id = client_id.into();
        Self {
            issuer: issuer.into(),
            audience: vec![client_id.clone()],
            client_id,
            redirect_uri: redirect_uri.into(),
            client_type,
            allowed_algorithms: vec![Algorithm::RS256, Algorithm::ES256, Algorithm::EdDSA],
            clock_skew: Duration::from_secs(30),
        }
    }

    /// Override accepted audiences.
    #[must_use]
    pub fn with_audience(mut self, audience: Vec<String>) -> Self {
        self.audience = audience;
        self
    }

    /// Override allowed algorithms.
    #[must_use]
    pub fn with_allowed_algorithms(mut self, algorithms: Vec<Algorithm>) -> Self {
        self.allowed_algorithms = algorithms;
        self
    }

    /// Override clock-skew tolerance.
    #[must_use]
    pub const fn with_clock_skew(mut self, clock_skew: Duration) -> Self {
        self.clock_skew = clock_skew;
        self
    }

    pub(super) fn validate(&self) -> Result<(), OidcError> {
        if self.clock_skew.as_secs() > 60 {
            return Err(OidcError::Configuration(
                "clock skew tolerance must be 60 seconds or less".into(),
            ));
        }
        let issuer = Url::parse(&self.issuer)
            .map_err(|error| OidcError::Configuration(format!("invalid issuer URL: {error}")))?;
        let redirect_uri = Url::parse(&self.redirect_uri)
            .map_err(|error| OidcError::Configuration(format!("invalid redirect URI: {error}")))?;

        if issuer.scheme() != "https" {
            return Err(OidcError::Configuration(
                "OIDC issuer must use HTTPS".into(),
            ));
        }
        if !is_allowed_redirect_uri(&redirect_uri) {
            return Err(OidcError::Configuration(
                "redirect URI must be HTTPS or a localhost development callback".into(),
            ));
        }
        if self.audience.is_empty() {
            return Err(OidcError::Configuration(
                "OIDC audience must not be empty".into(),
            ));
        }
        if self.allowed_algorithms.is_empty() {
            return Err(OidcError::Configuration(
                "OIDC allowed algorithms must not be empty".into(),
            ));
        }
        if self
            .allowed_algorithms
            .iter()
            .any(|algorithm| !is_approved_oidc_algorithm(*algorithm))
        {
            return Err(OidcError::Configuration(
                "OIDC allowed algorithms must be asymmetric RS256, ES256, or EdDSA".into(),
            ));
        }
        Ok(())
    }
}

const fn is_approved_oidc_algorithm(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::RS256 | Algorithm::ES256 | Algorithm::EdDSA
    )
}

fn is_allowed_redirect_uri(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() == "http"
        && let Some(host) = url.host_str()
    {
        return matches!(host, "localhost" | "127.0.0.1" | "::1");
    }
    false
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::Algorithm;

    use super::{OidcClientType, OidcConfig};

    #[test]
    fn oidc_config_rejects_empty_or_symmetric_algorithm_policy() {
        let empty = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_allowed_algorithms(Vec::new());
        assert!(empty.validate().is_err());

        let symmetric = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_allowed_algorithms(vec![Algorithm::HS256]);
        assert!(symmetric.validate().is_err());
    }

    #[test]
    fn oidc_config_allows_localhost_development_redirects() {
        let config = OidcConfig::new(
            "https://issuer.example",
            "client",
            "http://localhost:3000/callback",
            OidcClientType::Public,
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn oidc_config_rejects_invalid_boundaries() {
        let base = || {
            OidcConfig::new(
                "https://issuer.example",
                "client",
                "https://app.example/callback",
                OidcClientType::Confidential,
            )
        };

        assert!(
            OidcConfig::new(
                "not a url",
                "client",
                "https://app.example/callback",
                OidcClientType::Public,
            )
            .validate()
            .is_err()
        );
        assert!(
            OidcConfig::new(
                "http://issuer.example",
                "client",
                "https://app.example/callback",
                OidcClientType::Public,
            )
            .validate()
            .is_err()
        );
        assert!(
            OidcConfig::new(
                "https://issuer.example",
                "client",
                "not a redirect",
                OidcClientType::Public,
            )
            .validate()
            .is_err()
        );
        assert!(
            OidcConfig::new(
                "https://issuer.example",
                "client",
                "http://app.example/callback",
                OidcClientType::Public,
            )
            .validate()
            .is_err()
        );
        assert!(base().with_audience(Vec::new()).validate().is_err());
        assert!(
            base()
                .with_clock_skew(Duration::from_secs(61))
                .validate()
                .is_err()
        );
        assert!(base().validate().is_ok());
    }
}

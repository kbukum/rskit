use std::time::Duration;

use jsonwebtoken::Algorithm;
use reqwest::Url;
use rskit_util::SecretString;

use super::error::OidcError;

/// Default JWKS cache lifetime before a background re-fetch is allowed.
const DEFAULT_JWKS_CACHE_DURATION: Duration = Duration::from_mins(15);

/// Default upper bound on how stale cached JWKS may be while serving as a fallback for a failed
/// refresh. Beyond this bound the cached keys are considered untrustworthy and validation fails.
const DEFAULT_JWKS_MAX_STALENESS: Duration = Duration::from_hours(24);

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
    /// Client secret for confidential clients, used to authenticate token exchange.
    pub client_secret: Option<SecretString>,
    /// Exact redirect URI registered with the provider.
    pub redirect_uri: String,
    /// OIDC client type.
    pub client_type: OidcClientType,
    /// Scopes requested during authorization. Must include `openid`.
    pub scopes: Vec<String>,
    /// Accepted audience claim values.
    pub audience: Vec<String>,
    /// Accepted ID-token signing algorithms.
    pub allowed_algorithms: Vec<Algorithm>,
    /// Clock-skew tolerance for `exp` and `nbf`.
    pub clock_skew: Duration,
    /// Lifetime of a cached JWKS document before a re-fetch is allowed.
    pub jwks_cache_duration: Duration,
    /// Maximum age of cached JWKS that may still be served as a fallback when a refresh fails.
    ///
    /// While the provider is transiently unreachable, keys already cached remain usable up to this
    /// bound so a short outage does not fail every token validation. Beyond it, validation errors
    /// rather than trusting stale keys.
    pub jwks_max_staleness: Duration,
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
            client_secret: None,
            redirect_uri: redirect_uri.into(),
            client_type,
            scopes: vec!["openid".to_owned()],
            allowed_algorithms: vec![Algorithm::RS256, Algorithm::ES256, Algorithm::EdDSA],
            clock_skew: Duration::from_secs(30),
            jwks_cache_duration: DEFAULT_JWKS_CACHE_DURATION,
            jwks_max_staleness: DEFAULT_JWKS_MAX_STALENESS,
        }
    }

    /// Set the confidential-client secret.
    #[must_use]
    pub fn with_client_secret(mut self, client_secret: impl Into<String>) -> Self {
        self.client_secret = Some(SecretString::new(client_secret));
        self
    }

    /// Override the requested scopes. `openid` is required and enforced by validation.
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
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

    /// Override the JWKS cache lifetime.
    #[must_use]
    pub const fn with_jwks_cache_duration(mut self, jwks_cache_duration: Duration) -> Self {
        self.jwks_cache_duration = jwks_cache_duration;
        self
    }

    /// Override the maximum staleness allowed for cached JWKS served as a refresh fallback.
    #[must_use]
    pub const fn with_jwks_max_staleness(mut self, jwks_max_staleness: Duration) -> Self {
        self.jwks_max_staleness = jwks_max_staleness;
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
        if self.scopes.iter().all(|scope| scope != "openid") {
            return Err(OidcError::Configuration(
                "OIDC scopes must include 'openid'".into(),
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
        if matches!(self.client_type, OidcClientType::Public)
            && self
                .client_secret
                .as_ref()
                .is_some_and(|secret| !secret.is_empty())
        {
            return Err(OidcError::Configuration(
                "public OIDC clients must not carry a client secret".into(),
            ));
        }
        if self.jwks_max_staleness < self.jwks_cache_duration {
            return Err(OidcError::Configuration(
                "OIDC jwks_max_staleness must be at least jwks_cache_duration".into(),
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

    #[test]
    fn oidc_config_requires_openid_scope() {
        let config = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_scopes(vec!["profile".to_owned(), "email".to_owned()]);
        assert!(config.validate().is_err());

        let ok = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_scopes(vec!["openid".to_owned(), "email".to_owned()]);
        assert!(ok.validate().is_ok());
        assert_eq!(ok.scopes, vec!["openid".to_owned(), "email".to_owned()]);
    }

    #[test]
    fn oidc_config_rejects_secret_for_public_client() {
        let public_with_secret = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_client_secret("super-secret");
        assert!(public_with_secret.validate().is_err());

        let confidential_with_secret = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Confidential,
        )
        .with_client_secret("super-secret");
        assert!(confidential_with_secret.validate().is_ok());

        let public_without_secret = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        );
        assert!(public_without_secret.validate().is_ok());
    }

    #[test]
    fn oidc_config_rejects_staleness_below_cache_duration() {
        let base = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        )
        .with_jwks_cache_duration(Duration::from_mins(30));

        // A staleness bound tighter than the fresh TTL is nonsensical: the stale fallback would
        // never engage, so it is rejected.
        assert!(
            base.clone()
                .with_jwks_max_staleness(Duration::from_mins(10))
                .validate()
                .is_err()
        );
        // Equal or larger staleness is accepted.
        assert!(
            base.with_jwks_max_staleness(Duration::from_mins(30))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn oidc_config_defaults_and_overrides_jwks_cache_duration() {
        let default = OidcConfig::new(
            "https://issuer.example",
            "client",
            "https://app.example/callback",
            OidcClientType::Public,
        );
        assert_eq!(default.jwks_cache_duration, Duration::from_mins(15));
        assert_eq!(default.scopes, vec!["openid".to_owned()]);

        let overridden = default.with_jwks_cache_duration(Duration::from_mins(1));
        assert_eq!(overridden.jwks_cache_duration, Duration::from_mins(1));
    }
}

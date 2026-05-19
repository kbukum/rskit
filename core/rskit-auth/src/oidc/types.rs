use std::fmt;

use base64::Engine;
use serde::Deserialize;
use sha2::Digest;

/// OIDC discovery document.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcProviderMetadata {
    /// Issuer URL.
    pub issuer: String,
    /// Authorization endpoint.
    pub authorization_endpoint: String,
    /// Token endpoint.
    pub token_endpoint: String,
    /// JWKS endpoint.
    pub jwks_uri: String,
    /// Userinfo endpoint.
    pub userinfo_endpoint: Option<String>,
    /// Supported response types.
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    /// Supported PKCE code challenge methods.
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    /// Supported ID-token signing algorithms.
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,
}

/// Built authorization request plus correlated anti-CSRF state.
#[derive(Clone)]
pub struct OidcAuthorizationRequest {
    /// Fully rendered authorization URL.
    pub url: String,
    /// Anti-CSRF state value.
    pub state: String,
    /// Nonce that must be echoed in the ID token.
    pub nonce: String,
    /// PKCE verifier/challenge pair.
    pub pkce: Option<PkcePair>,
}

impl fmt::Debug for OidcAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcAuthorizationRequest")
            .field("url", &"<redacted>")
            .field("state", &"<redacted>")
            .field("nonce", &"<redacted>")
            .field("pkce", &self.pkce.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Token exchange parameters validated against the authorization request state.
#[derive(Clone)]
pub struct OidcTokenExchangeRequest {
    /// Token endpoint discovered from the provider.
    pub token_endpoint: String,
    /// Authorization code from the callback.
    pub code: String,
    /// Exact redirect URI configured for the client.
    pub redirect_uri: String,
    /// Original anti-CSRF state.
    pub state: String,
    /// Optional PKCE verifier. Required for public clients.
    pub code_verifier: Option<String>,
}

impl fmt::Debug for OidcTokenExchangeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcTokenExchangeRequest")
            .field("token_endpoint", &self.token_endpoint)
            .field("code", &"<redacted>")
            .field("redirect_uri", &self.redirect_uri)
            .field("state", &"<redacted>")
            .field(
                "code_verifier",
                &self.code_verifier.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// PKCE verifier/challenge pair.
#[derive(Clone)]
pub struct PkcePair {
    /// High-entropy verifier.
    pub verifier: String,
    /// SHA-256 based challenge.
    pub challenge: String,
    /// Challenge method, always `S256`.
    pub method: &'static str,
}

impl fmt::Debug for PkcePair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PkcePair")
            .field("verifier", &"<redacted>")
            .field("challenge", &"<redacted>")
            .field("method", &self.method)
            .finish()
    }
}

impl PkcePair {
    /// Generate a PKCE verifier/challenge pair.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        rand::fill(&mut bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(sha2::Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
            method: "S256",
        }
    }
}

/// Claims extracted from a validated OIDC ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcClaims {
    /// Subject identifier.
    pub sub: String,
    /// Issuer identifier.
    pub iss: String,
    /// Audience values.
    pub aud: Vec<String>,
    /// Expiration timestamp (seconds since epoch).
    pub exp: u64,
    /// Issued-at timestamp.
    pub iat: u64,
    /// Not-before timestamp, if present.
    pub nbf: Option<u64>,
    /// OIDC nonce claim, if present.
    pub nonce: Option<String>,
    /// User email, if present.
    pub email: Option<String>,
    /// Whether the provider verified the email.
    pub email_verified: Option<bool>,
    /// Human-readable display name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RawOidcClaims {
    pub(super) sub: String,
    pub(super) iss: String,
    pub(super) aud: Audiences,
    pub(super) exp: u64,
    pub(super) iat: Option<u64>,
    pub(super) nbf: Option<u64>,
    pub(super) nonce: Option<String>,
    pub(super) email: Option<String>,
    pub(super) email_verified: Option<bool>,
    pub(super) name: Option<String>,
}

/// User profile returned by the OIDC userinfo endpoint.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OidcUserInfo {
    /// Subject identifier.
    pub sub: String,
    /// User email, if present.
    pub email: Option<String>,
    /// Whether the provider verified the email.
    pub email_verified: Option<bool>,
    /// Human-readable name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum Audiences {
    One(String),
    Many(Vec<String>),
}

impl Audiences {
    pub(super) fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OidcAuthorizationRequest, OidcTokenExchangeRequest, PkcePair};

    #[test]
    fn oidc_debug_masks_callback_secrets() {
        let pkce = PkcePair {
            verifier: "verifier-secret".into(),
            challenge: "challenge-secret".into(),
            method: "S256",
        };
        let authorization = OidcAuthorizationRequest {
            url: "https://issuer/authorize?state=state-secret&nonce=nonce-secret".into(),
            state: "state-secret".into(),
            nonce: "nonce-secret".into(),
            pkce: Some(pkce.clone()),
        };
        let exchange = OidcTokenExchangeRequest {
            token_endpoint: "https://issuer/token".into(),
            code: "code-secret".into(),
            redirect_uri: "https://app/callback".into(),
            state: "state-secret".into(),
            code_verifier: Some(pkce.verifier.clone()),
        };

        let formatted = format!("{authorization:?} {exchange:?} {pkce:?}");

        for secret in [
            "state-secret",
            "nonce-secret",
            "verifier-secret",
            "challenge-secret",
            "code-secret",
        ] {
            assert!(!formatted.contains(secret), "{formatted}");
        }
    }
}

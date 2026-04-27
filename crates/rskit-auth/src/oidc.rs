//! OpenID Connect (OIDC) support — discovery, token validation, userinfo.
//!
//! # Status
//! This module provides a scaffold for OIDC integration.
//! Full implementation is tracked in issue #20.

/// OIDC provider configuration.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Issuer URL (e.g. `https://accounts.google.com`).
    pub issuer: String,
    /// Client ID registered with the provider.
    pub client_id: String,
    /// Expected audience claim.
    pub audience: String,
}

/// Validates an OIDC ID token against the provider's JWKS.
///
/// # Errors
/// Returns an error if the token is invalid, expired, or the provider is unreachable.
///
/// # TODO
/// - Implement JWKS fetching and caching (see issue #20)
/// - Implement token validation against fetched keys
/// - Implement userinfo endpoint call
#[allow(clippy::unused_async)] // Stub — will use await once JWKS fetching is implemented (#20)
pub async fn validate_id_token(
    _config: &OidcConfig,
    _id_token: &str,
) -> Result<OidcClaims, OidcError> {
    Err(OidcError::NotImplemented)
}

/// Claims extracted from a validated OIDC ID token.
#[derive(Debug, Clone)]
pub struct OidcClaims {
    /// Subject identifier (unique user ID from the provider).
    pub sub: String,
    /// User's email address, if provided by the provider.
    pub email: Option<String>,
    /// User's display name, if provided by the provider.
    pub name: Option<String>,
}

/// Errors from OIDC operations.
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    /// OIDC validation is not yet implemented.
    #[error("OIDC not yet implemented — see issue #20")]
    NotImplemented,
    /// The token could not be validated.
    #[error("invalid token: {0}")]
    InvalidToken(String),
    /// The OIDC provider could not be reached.
    #[error("provider unreachable: {0}")]
    ProviderUnreachable(String),
}

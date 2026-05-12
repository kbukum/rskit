/// Errors from OIDC operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OidcError {
    /// Configuration is invalid or unsafe.
    #[error("invalid OIDC configuration: {0}")]
    Configuration(String),
    /// Discovery failed or returned an invalid document.
    #[error("OIDC discovery failed: {0}")]
    Discovery(String),
    /// The token could not be validated.
    #[error("invalid token: {0}")]
    InvalidToken(String),
    /// The OIDC provider could not be reached.
    #[error("provider unreachable: {0}")]
    ProviderUnreachable(String),
    /// Callback state mismatched the original request.
    #[error("OIDC state mismatch")]
    StateMismatch,
    /// Public clients must supply PKCE verifiers.
    #[error("PKCE verifier is required for public clients")]
    MissingPkce,
    /// Nonce mismatched the validated ID token.
    #[error("OIDC nonce mismatch")]
    NonceMismatch,
    /// The provider or token selected an unsupported algorithm.
    #[error("unsupported OIDC signing algorithm: {0}")]
    UnsupportedAlgorithm(String),
    /// A required claim is missing.
    #[error("missing required OIDC claim: {0}")]
    MissingClaim(String),
}

//! Authentication — password hashing, API key management, request-context helpers,
//! and optional JWT/OIDC support.
//!
//! Request middleware is fail-closed by default.
//! [`BearerAuthLayer`] reads only `Authorization: Bearer ...` headers
//! and emits a neutral `WWW-Authenticate: Bearer` challenge on rejected requests;
//! applications can add realm/scope-specific challenge policy at their HTTP boundary if needed.

#![warn(missing_docs)]

/// API key generation, hashing, validation, and rotation with grace periods.
pub mod apikey;
/// Header-only bearer authentication middleware.
mod bearer;
/// Auth claims stored in request extensions / task-locals.
pub mod context;
/// JWT sign/verify service.
#[cfg(feature = "jwt")]
pub mod jwt;
/// OpenID Connect (OIDC) support — discovery, token validation, userinfo.
#[cfg(feature = "oidc")]
pub mod oidc;
/// Typed request authentication outcomes.
pub mod outcome;
/// Password hashing and reset-token generation.
pub mod password;
/// Core `TokenValidator` and `TokenGenerator` traits.
pub mod traits;

pub use bearer::{BearerAuthLayer, BearerAuthService};
pub use context::AuthClaims;
#[cfg(feature = "jwt")]
pub use jwt::{
    ACCESS_TOKEN_TYPE, AsymmetricAlgorithm, JwtAlgorithm, JwtCodec, JwtConfig, JwtHeader,
    JwtKeyMaterial, JwtService, KeyPair, REFRESH_TOKEN_TYPE,
};
#[cfg(feature = "oidc")]
pub use oidc::{
    OidcAuthorizationRequest, OidcClaims, OidcClient, OidcClientType, OidcConfig, OidcError,
    OidcHttpClient, OidcProviderMetadata, OidcTokenExchangeRequest, OidcUserInfo, PkcePair,
    ReqwestOidcHttpClient, validate_id_token,
};
pub use outcome::{AuthOutcome, MissingCredentialPolicy};
pub use password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
pub use traits::{TokenGenerator, TokenValidator};

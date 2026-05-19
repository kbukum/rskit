//! Authentication — JWT, OIDC, password hashing, API key management, request-context helpers.

#![warn(missing_docs)]

/// API key generation, hashing, validation, and rotation with grace periods.
pub mod apikey;
/// Header-only bearer authentication middleware.
mod bearer;
/// Auth claims stored in request extensions / task-locals.
pub mod context;
/// JWT sign/verify service.
pub mod jwt;
/// OpenID Connect (OIDC) support — discovery, token validation, userinfo.
pub mod oidc;
/// Typed request authentication outcomes.
pub mod outcome;
/// Password hashing and reset-token generation.
pub mod password;
/// Core `TokenValidator` and `TokenGenerator` traits.
pub mod traits;

pub use bearer::{BearerAuthLayer, BearerAuthService};
pub use context::AuthClaims;
pub use jwt::{AsymmetricAlgorithm, JwtAlgorithm, JwtConfig, JwtKeyMaterial, JwtService, KeyPair};
pub use outcome::{AuthOutcome, MissingCredentialPolicy};
pub use password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
pub use traits::{TokenGenerator, TokenValidator};

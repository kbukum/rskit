//! Authentication — JWT, OIDC, password hashing, API key management, request-context helpers.

#![warn(missing_docs)]

/// JWT algorithm typestate — prevents algorithm confusion attacks at compile time.
pub mod algo;
/// API key generation, hashing, validation, and rotation with grace periods.
pub mod apikey;
/// Auth claims stored in request extensions / task-locals.
pub mod context;
/// JWT sign/verify service.
pub mod jwt;
/// Password hashing and reset-token generation.
pub mod password;
/// Core `TokenValidator` and `TokenGenerator` traits.
pub mod traits;
/// OpenID Connect (OIDC) support — discovery, token validation, userinfo.
pub mod oidc;

pub use context::AuthClaims;
pub use jwt::{JwtConfig, JwtService};
pub use password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
pub use traits::{TokenGenerator, TokenValidator};

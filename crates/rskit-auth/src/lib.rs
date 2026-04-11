//! Authentication — JWT, OIDC, password hashing, API key management, request-context helpers.

#![warn(missing_docs)]

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

pub use context::AuthClaims;
pub use jwt::{JwtConfig, JwtService};
pub use password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
pub use traits::{TokenGenerator, TokenValidator};

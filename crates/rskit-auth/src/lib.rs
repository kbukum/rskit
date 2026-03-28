//! Authentication — JWT, OIDC, password hashing, request-context helpers.

#![warn(missing_docs)]

/// Core `TokenValidator` and `TokenGenerator` traits.
pub mod traits;
/// JWT sign/verify service.
pub mod jwt;
/// Password hashing and reset-token generation.
pub mod password;
/// Auth claims stored in request extensions / task-locals.
pub mod context;

pub use traits::{TokenGenerator, TokenValidator};
pub use jwt::{JwtConfig, JwtService};
pub use password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
pub use context::AuthClaims;

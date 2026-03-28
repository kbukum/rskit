//! Authentication — JWT, OIDC, password hashing, request-context helpers.

#![warn(missing_docs)]

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

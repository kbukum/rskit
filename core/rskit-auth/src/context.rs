//! Stores and retrieves auth claims from request context.

/// Wrapper that carries typed claims in an axum request extension.
///
/// # Usage (axum)
///
/// ```rust,ignore
/// use rskit_auth::AuthClaims;
///
/// async fn my_handler(
///     axum::Extension(AuthClaims(claims)): axum::Extension<AuthClaims<MyClaims>>,
/// ) { /* … */ }
/// ```
#[derive(Debug, Clone)]
pub struct AuthClaims<C>(pub C);

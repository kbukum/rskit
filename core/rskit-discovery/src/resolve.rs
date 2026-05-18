//! Bootstrap-time address resolution utilities.
//!
//! [`resolve_addr`] resolves a service name to a `(host, port)` pair using the
//! [`Discovery`] trait. This is intended for one-shot infrastructure resolution
//! at startup — before connection pools are created — not for runtime
//! load balancing.

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::traits::Discovery;

/// Resolve a service name to a `(host, port)` pair via service discovery.
///
/// Returns the first healthy instance's address and port. Use this at bootstrap
/// time to resolve infrastructure addresses (database, redis, kafka, etc.)
/// before connection pools are created.
pub async fn resolve_addr(disc: &dyn Discovery, service: &str) -> AppResult<(String, u16)> {
    let instances = disc.resolve(service).await?;

    let inst = instances
        .iter()
        .find(|instance| instance.healthy)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("resolve \"{service}\": no healthy instances found"),
            )
        })?;

    Ok((inst.address.clone(), inst.port))
}

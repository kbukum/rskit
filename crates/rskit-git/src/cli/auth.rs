//! CLI auth helpers.

use crate::auth::TransportAuth;
use rskit_errors::AppResult;

/// Placeholder hook for future CLI auth composition.
#[allow(dead_code)]
pub fn apply_transport(_auth: Option<&TransportAuth>) -> AppResult<()> {
    Ok(())
}

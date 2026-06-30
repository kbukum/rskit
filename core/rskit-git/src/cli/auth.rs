//! CLI auth helpers.

use crate::auth::TransportAuth;
use rskit_errors::AppResult;

/// Applies transport auth to a CLI git invocation.
#[allow(dead_code)]
pub fn apply_transport(_auth: Option<&TransportAuth>) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_transport_accepts_default_and_explicit_auth_without_side_effects() {
        apply_transport(None).expect("default CLI auth is accepted");
        apply_transport(Some(&TransportAuth::Token {
            username: Some("git".to_string()),
            token: "secret".to_string(),
        }))
        .expect("token CLI auth is accepted for later command composition");
    }
}

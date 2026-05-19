//! Typed authentication outcomes for request middleware.

/// Policy for requests that do not include credentials.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MissingCredentialPolicy {
    /// Reject requests with missing credentials.
    #[default]
    RejectMissing,
    /// Allow requests with missing credentials and expose [`AuthOutcome::Missing`].
    AcceptMissing,
}

/// Result of request authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthOutcome<C> {
    /// Credentials were present and valid.
    Authenticated(C),
    /// Credentials were absent and the middleware was configured with
    /// [`MissingCredentialPolicy::AcceptMissing`].
    Missing,
}

impl<C> AuthOutcome<C> {
    /// Return the authenticated claims when present.
    #[must_use]
    pub const fn claims(&self) -> Option<&C> {
        match self {
            Self::Authenticated(claims) => Some(claims),
            Self::Missing => None,
        }
    }

    /// Return `true` when credentials were absent and accepted by policy.
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthOutcome, MissingCredentialPolicy};

    #[test]
    fn missing_policy_rejects_missing_by_default() {
        assert_eq!(
            MissingCredentialPolicy::default(),
            MissingCredentialPolicy::RejectMissing
        );
    }

    #[test]
    fn auth_outcome_exposes_claims_only_when_authenticated() {
        let authenticated = AuthOutcome::Authenticated("claims");
        assert_eq!(authenticated.claims(), Some(&"claims"));
        assert!(!authenticated.is_missing());

        let missing = AuthOutcome::<&str>::Missing;
        assert_eq!(missing.claims(), None);
        assert!(missing.is_missing());
    }
}

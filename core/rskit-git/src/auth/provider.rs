//! Auth provider abstraction and a small toolkit of generic implementations.
//!
//! [`AuthProvider`] is the seam git backends use to resolve transport and
//! signing configuration at call time. The provided implementations are
//! forge-agnostic building blocks: callers (applications) decide policy such as
//! which environment variables carry a token by composing them, typically via a
//! [`ChainAuthProvider`]. No provider here hard-codes a forge-specific name.

use std::sync::Arc;

use rskit_errors::AppResult;
use rskit_util::{SecretString, env};

use super::{SigningConfig, TransportAuth};

/// Default username used for token-as-password HTTP basic auth when the caller
/// does not override it. This is a generic transport convention for carrying a
/// token in the username field, not a forge identifier; callers override it per
/// remote via [`TransportAuth::Token`].
pub const DEFAULT_TOKEN_USERNAME: &str = "x-access-token";

/// Supplies transport and signing configuration for git backends.
pub trait AuthProvider: Send + Sync {
    /// Resolves transport auth for an optional remote name.
    ///
    /// Returns `Ok(None)` when this provider has nothing to offer, so a
    /// [`ChainAuthProvider`] can fall through to the next provider.
    fn transport_auth(&self, remote: Option<&str>) -> AppResult<Option<TransportAuth>>;

    /// Resolves signing configuration for commit-producing operations.
    ///
    /// Defaults to `Ok(None)`; the seam allows wiring signing later without
    /// touching existing providers.
    fn signing_config(&self) -> AppResult<Option<SigningConfig>> {
        Ok(None)
    }
}

impl AuthProvider for Arc<dyn AuthProvider> {
    fn transport_auth(&self, remote: Option<&str>) -> AppResult<Option<TransportAuth>> {
        (**self).transport_auth(remote)
    }

    fn signing_config(&self) -> AppResult<Option<SigningConfig>> {
        (**self).signing_config()
    }
}

/// Provider that offers nothing, deferring to the backend transport default.
///
/// Behavior-preserving: a repository opened with this provider authenticates
/// exactly as it did before the auth seam was wired.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultAuthProvider;

impl AuthProvider for DefaultAuthProvider {
    fn transport_auth(&self, _remote: Option<&str>) -> AppResult<Option<TransportAuth>> {
        Ok(None)
    }
}

/// Provider that always returns a fixed [`TransportAuth`].
///
/// Use for explicitly configured credentials (an SSH key, the SSH agent, or a
/// caller-held token).
#[derive(Debug, Clone)]
pub struct StaticAuthProvider {
    auth: TransportAuth,
}

impl StaticAuthProvider {
    /// Create a provider that always yields `auth`.
    #[must_use]
    pub fn new(auth: TransportAuth) -> Self {
        Self { auth }
    }
}

impl AuthProvider for StaticAuthProvider {
    fn transport_auth(&self, _remote: Option<&str>) -> AppResult<Option<TransportAuth>> {
        Ok(Some(self.auth.clone()))
    }
}

/// Provider that reads a token from the first present, non-empty environment
/// variable in an ordered list and exposes it as token-as-password auth.
///
/// The variable names are supplied by the caller — this type owns the mechanism
/// (read env → build [`TransportAuth::Token`]), never the policy of which
/// variables or which forge. Returns `Ok(None)` when none of the variables are
/// set, so it is harmless to inject unconditionally in a chain.
#[derive(Debug, Clone)]
pub struct EnvTokenAuthProvider {
    vars: Vec<String>,
    username: String,
}

impl EnvTokenAuthProvider {
    /// Create a provider that reads the single environment variable `name`.
    #[must_use]
    pub fn with_var(name: impl Into<String>) -> Self {
        Self {
            vars: vec![name.into()],
            username: DEFAULT_TOKEN_USERNAME.to_string(),
        }
    }

    /// Create a provider that reads the first present variable in `names`.
    #[must_use]
    pub fn with_vars<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            vars: names.into_iter().map(Into::into).collect(),
            username: DEFAULT_TOKEN_USERNAME.to_string(),
        }
    }

    /// Override the basic-auth username paired with the token.
    #[must_use]
    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }
}

impl AuthProvider for EnvTokenAuthProvider {
    fn transport_auth(&self, _remote: Option<&str>) -> AppResult<Option<TransportAuth>> {
        Ok(self
            .vars
            .iter()
            .find_map(|var| env::get_non_empty(var))
            .map(|token| TransportAuth::Token {
                username: Some(self.username.clone()),
                token: SecretString::new(token),
            }))
    }
}

/// Provider that consults an ordered list of providers; the first `Some` wins,
/// for both transport and signing resolution.
#[derive(Clone, Default)]
pub struct ChainAuthProvider {
    providers: Vec<Arc<dyn AuthProvider>>,
}

impl ChainAuthProvider {
    /// Create a chain from an ordered list of providers.
    #[must_use]
    pub fn new(providers: Vec<Arc<dyn AuthProvider>>) -> Self {
        Self { providers }
    }

    /// Append a provider to the end of the chain.
    #[must_use]
    pub fn with(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.providers.push(provider);
        self
    }
}

impl std::fmt::Debug for ChainAuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainAuthProvider")
            .field("providers", &self.providers.len())
            .finish()
    }
}

impl AuthProvider for ChainAuthProvider {
    fn transport_auth(&self, remote: Option<&str>) -> AppResult<Option<TransportAuth>> {
        for provider in &self.providers {
            if let Some(auth) = provider.transport_auth(remote)? {
                return Ok(Some(auth));
            }
        }
        Ok(None)
    }

    fn signing_config(&self) -> AppResult<Option<SigningConfig>> {
        for provider in &self.providers {
            if let Some(config) = provider.signing_config()? {
                return Ok(Some(config));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cargo sets `CARGO_PKG_NAME` for the test process, so we can assert the
    // present-var path without the (edition-2024-unsafe) `std::env::set_var`.
    const PRESENT_VAR: &str = "CARGO_PKG_NAME";
    const PRESENT_VALUE: &str = "rskit-git";
    const ABSENT_VAR: &str = "RSKIT_GIT_AUTH_TEST_ABSENT_VAR_9F3A";

    #[test]
    fn default_provider_offers_nothing() {
        let provider = DefaultAuthProvider;
        assert_eq!(provider.transport_auth(None).expect("resolve"), None);
        assert!(provider.signing_config().expect("resolve").is_none());
    }

    #[test]
    fn static_provider_returns_fixed_transport() {
        let provider = StaticAuthProvider::new(TransportAuth::SshAgent {
            username: "git".to_string(),
        });
        assert_eq!(
            provider.transport_auth(Some("origin")).expect("resolve"),
            Some(TransportAuth::SshAgent {
                username: "git".to_string(),
            })
        );
    }

    #[test]
    fn env_token_provider_reads_present_variable() {
        let provider = EnvTokenAuthProvider::with_vars([ABSENT_VAR, PRESENT_VAR]);
        let auth = provider.transport_auth(None).expect("resolve");
        assert_eq!(
            auth,
            Some(TransportAuth::Token {
                username: Some(DEFAULT_TOKEN_USERNAME.to_string()),
                token: SecretString::new(PRESENT_VALUE),
            })
        );
    }

    #[test]
    fn env_token_provider_honors_username_override() {
        let provider = EnvTokenAuthProvider::with_var(PRESENT_VAR).with_username("token-user");
        let auth = provider.transport_auth(None).expect("resolve");
        assert_eq!(
            auth,
            Some(TransportAuth::Token {
                username: Some("token-user".to_string()),
                token: SecretString::new(PRESENT_VALUE),
            })
        );
    }

    #[test]
    fn env_token_provider_absent_variable_is_none() {
        let provider = EnvTokenAuthProvider::with_var(ABSENT_VAR);
        assert_eq!(provider.transport_auth(None).expect("resolve"), None);
    }

    #[test]
    fn chain_returns_first_some() {
        let chain = ChainAuthProvider::new(vec![
            Arc::new(EnvTokenAuthProvider::with_var(ABSENT_VAR)),
            Arc::new(EnvTokenAuthProvider::with_var(PRESENT_VAR)),
            Arc::new(StaticAuthProvider::new(TransportAuth::SshAgent {
                username: "unused".to_string(),
            })),
        ]);
        let auth = chain.transport_auth(None).expect("resolve");
        assert_eq!(
            auth,
            Some(TransportAuth::Token {
                username: Some(DEFAULT_TOKEN_USERNAME.to_string()),
                token: SecretString::new(PRESENT_VALUE),
            })
        );
    }

    #[test]
    fn chain_falls_through_to_none() {
        let chain = ChainAuthProvider::new(vec![
            Arc::new(EnvTokenAuthProvider::with_var(ABSENT_VAR)),
            Arc::new(DefaultAuthProvider),
        ]);
        assert_eq!(chain.transport_auth(None).expect("resolve"), None);
        assert!(chain.signing_config().expect("resolve").is_none());
    }
}

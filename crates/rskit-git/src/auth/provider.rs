//! Auth provider abstraction.

use rskit_errors::AppResult;

use super::{SigningConfig, TransportAuth};

/// Supplies transport and signing configuration for git backends.
pub trait AuthProvider: Send + Sync {
    /// Resolves transport auth for an optional remote name.
    fn transport_auth(&self, remote: Option<&str>) -> AppResult<Option<TransportAuth>>;

    /// Resolves signing configuration for commit-producing operations.
    fn signing_config(&self) -> AppResult<Option<SigningConfig>>;
}

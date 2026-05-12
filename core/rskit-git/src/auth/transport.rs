//! Transport authentication types.

use std::path::PathBuf;

/// Supported repository transport authentication strategies.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum TransportAuth {
    /// Use default environment-driven authentication.
    #[default]
    Default,
    /// Use a username/password combination.
    UsernamePassword {
        /// Login username.
        username: String,
        /// Login password.
        password: String,
    },
    /// Use an HTTP token.
    Token {
        /// Optional username override.
        username: Option<String>,
        /// Token or password value.
        token: String,
    },
    /// Use explicit SSH key material.
    SshKey {
        /// SSH username.
        username: String,
        /// Optional public key path.
        public_key: Option<PathBuf>,
        /// Private key path.
        private_key: PathBuf,
        /// Optional passphrase.
        passphrase: Option<String>,
    },
    /// Use the local SSH agent.
    SshAgent {
        /// SSH username.
        username: String,
    },
}

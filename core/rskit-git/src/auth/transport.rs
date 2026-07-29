//! Transport authentication types.

use std::path::PathBuf;

use rskit_util::SecretString;

/// Supported repository transport authentication strategies.
///
/// Credential-bearing fields are wrapped in [`SecretString`] so tokens and
/// passwords never leak through `Debug`, `Display`, or serialization; access the
/// plaintext intentionally with [`SecretString::expose`] only at the point it is
/// handed to `git2`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        password: SecretString,
    },
    /// Use an HTTP token.
    Token {
        /// Optional username override; defaults to [`DEFAULT_TOKEN_USERNAME`](super::DEFAULT_TOKEN_USERNAME) when `None`.
        username: Option<String>,
        /// Token or password value.
        token: SecretString,
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
        passphrase: Option<SecretString>,
    },
    /// Use the local SSH agent.
    SshAgent {
        /// SSH username.
        username: String,
    },
}

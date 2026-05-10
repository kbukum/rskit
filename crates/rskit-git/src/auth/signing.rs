//! Commit signing types.

use std::path::PathBuf;

/// Supported commit signing configurations.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SigningConfig {
    /// Sign commits with GPG.
    Gpg {
        /// Optional key identifier.
        key_id: Option<String>,
    },
    /// Sign commits with SSH.
    Ssh {
        /// Path to the SSH signing key.
        key_path: PathBuf,
    },
    /// Sign commits with an X.509 certificate.
    X509 {
        /// Path to the certificate file.
        cert_path: PathBuf,
        /// Path to the private key file.
        key_path: PathBuf,
    },
}

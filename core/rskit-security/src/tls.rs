//! TLS material and verification policy.

use rskit_errors::{AppError, AppResult};

/// Minimum TLS protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum TlsVersion {
    /// TLS 1.2 minimum.
    #[default]
    Tls12,
    /// TLS 1.3 minimum.
    Tls13,
}

/// TLS material and peer-verification settings shared by transport crates.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct TlsConfig {
    /// Disables peer certificate verification. Intended only for controlled local/test use.
    pub skip_verify: bool,
    /// Path to a PEM-encoded CA bundle used to verify peers.
    pub ca_file: Option<String>,
    /// Path to a PEM-encoded certificate.
    pub cert_file: Option<String>,
    /// Path to a PEM-encoded private key paired with `cert_file`.
    pub key_file: Option<String>,
    /// Override the certificate server name.
    pub server_name: Option<String>,
    /// Minimum TLS version. Defaults to TLS 1.2 while rustls prefers TLS 1.3.
    #[serde(default)]
    pub min_version: TlsVersion,
}

impl TlsConfig {
    /// Return `true` when any TLS setting is configured.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.skip_verify
            || self.ca_file.as_ref().is_some_and(|value| !value.is_empty())
            || self
                .cert_file
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            || self
                .key_file
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            || self
                .server_name
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            || self.min_version != TlsVersion::Tls12
    }

    /// Validate TLS configuration consistency.
    ///
    /// # Errors
    /// Returns an error when certificate/key material is incomplete or empty.
    pub fn validate(&self) -> AppResult<()> {
        validate_non_empty("ca_file", self.ca_file.as_deref())?;
        validate_non_empty("cert_file", self.cert_file.as_deref())?;
        validate_non_empty("key_file", self.key_file.as_deref())?;
        validate_non_empty("server_name", self.server_name.as_deref())?;

        if self.cert_file.is_some() != self.key_file.is_some() {
            return Err(AppError::invalid_input(
                "tls",
                "cert_file and key_file must be provided together",
            ));
        }
        Ok(())
    }
}

fn validate_non_empty(field: &str, value: Option<&str>) -> AppResult<()> {
    if value.is_some_and(str::is_empty) {
        return Err(AppError::invalid_input(
            field,
            format!("{field} must not be empty when provided"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cert_and_key_must_be_configured_together() {
        let config = TlsConfig {
            cert_file: Some("client.pem".to_string()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn detects_enabled_tls_settings() {
        let config = TlsConfig {
            ca_file: Some("ca.pem".to_string()),
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn default_config_is_disabled_and_valid() {
        let config = TlsConfig::default();

        assert!(!config.is_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn cert_and_key_pair_is_valid() {
        let config = TlsConfig {
            cert_file: Some("client.pem".to_string()),
            key_file: Some("client.key".to_string()),
            ..Default::default()
        };

        assert!(config.validate().is_ok());
        assert!(config.is_enabled());
    }

    #[test]
    fn rejects_empty_optional_values() {
        for config in [
            TlsConfig {
                ca_file: Some(String::new()),
                ..Default::default()
            },
            TlsConfig {
                cert_file: Some(String::new()),
                key_file: Some("key.pem".to_string()),
                ..Default::default()
            },
            TlsConfig {
                key_file: Some(String::new()),
                cert_file: Some("cert.pem".to_string()),
                ..Default::default()
            },
            TlsConfig {
                server_name: Some(String::new()),
                ..Default::default()
            },
        ] {
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn tls13_minimum_counts_as_enabled() {
        let config = TlsConfig {
            min_version: TlsVersion::Tls13,
            ..Default::default()
        };

        assert!(config.is_enabled());
        assert!(config.validate().is_ok());
    }
}

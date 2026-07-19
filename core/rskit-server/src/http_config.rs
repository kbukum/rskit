use std::time::Duration;

pub use rskit_http::CorsPolicy;
use rskit_security::TlsConfig;
use serde::Deserialize;
use validator::{Validate, ValidationError, ValidationErrors};

/// HTTP server configuration owned by `rskit-server`.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpServerConfig {
    /// Bind address (default: `0.0.0.0`).
    #[serde(default = "HttpServerConfig::default_host")]
    pub host: String,

    /// Bind port (default: `8080`).
    #[serde(default = "HttpServerConfig::default_port")]
    pub port: u16,

    /// Maximum time to wait for a request header (default: 30 s).
    #[serde(default = "HttpServerConfig::default_timeout")]
    pub read_timeout: Duration,

    /// Idle keep-alive connection timeout (default: 60 s).
    #[serde(default = "HttpServerConfig::default_idle_timeout")]
    pub idle_timeout: Duration,

    /// End-to-end request timeout enforced by the HTTP middleware stack (default: 30 s).
    #[serde(default = "HttpServerConfig::default_timeout")]
    pub request_timeout: Duration,

    /// Maximum accepted request body size in bytes (default: 2 MiB).
    #[serde(default = "HttpServerConfig::default_max_body_bytes")]
    pub max_body_bytes: usize,

    /// Enable HTTP/2 cleartext (h2c) on the same port (default: `true`).
    #[serde(default = "HttpServerConfig::default_h2c")]
    pub enable_h2c: bool,

    /// Optional CORS policy.
    pub cors: Option<CorsPolicy>,

    /// Optional direct HTTPS serving configuration.
    ///
    /// When configured, rustls is used with TLS 1.3 preferred
    /// and TLS 1.2 as the minimum protocol floor unless a stricter minimum is configured.
    pub tls: Option<TlsConfig>,
}

impl Validate for HttpServerConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if self.port == 0 {
            errors.add("port", ValidationError::new("range"));
        }

        if let Some(cors) = &self.cors
            && let Err(error) = cors.validate()
        {
            let mut validation_error = ValidationError::new("invalid_cors");
            validation_error.message = Some(error.to_string().into());
            errors.add("cors", validation_error);
        }

        if let Some(tls) = &self.tls
            && let Err(error) = validate_http_tls_config(tls)
        {
            let mut validation_error = ValidationError::new("invalid_tls");
            validation_error.message = Some(error.to_string().into());
            errors.add("tls", validation_error);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn validate_http_tls_config(tls: &TlsConfig) -> rskit_errors::AppResult<()> {
    tls.validate()?;
    if tls.cert_file.is_none() || tls.key_file.is_none() {
        return Err(rskit_errors::AppError::invalid_input(
            "tls",
            "tls.cert_file and tls.key_file are required for HTTPS serving",
        ));
    }
    if tls.skip_verify || tls.ca_file.is_some() || tls.server_name.is_some() {
        return Err(rskit_errors::AppError::invalid_input(
            "tls",
            "skip_verify, ca_file, and server_name are client-side TLS settings and are not used by HTTPS serving",
        ));
    }
    Ok(())
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: Self::default_host(),
            port: Self::default_port(),
            read_timeout: Self::default_timeout(),
            idle_timeout: Self::default_idle_timeout(),
            request_timeout: Self::default_timeout(),
            max_body_bytes: Self::default_max_body_bytes(),
            enable_h2c: Self::default_h2c(),
            cors: None,
            tls: None,
        }
    }
}

impl HttpServerConfig {
    fn default_host() -> String {
        "0.0.0.0".to_string()
    }

    fn default_port() -> u16 {
        8080
    }

    fn default_timeout() -> Duration {
        Duration::from_secs(30)
    }

    fn default_idle_timeout() -> Duration {
        Duration::from_secs(60)
    }

    fn default_max_body_bytes() -> usize {
        2 * 1024 * 1024
    }

    fn default_h2c() -> bool {
        true
    }

    /// Returns the bind address as `host:port` (or `[host]:port` for IPv6).
    #[must_use]
    pub fn bind_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;
    use rskit_security::TlsVersion;

    use super::*;

    #[test]
    fn bind_addr_wraps_ipv6_and_validation_rejects_invalid_port() {
        let config = HttpServerConfig {
            host: "::1".to_string(),
            port: 443,
            ..Default::default()
        };
        assert_eq!(config.bind_addr(), "[::1]:443");
        assert_eq!(HttpServerConfig::default().bind_addr(), "0.0.0.0:8080");

        let invalid = HttpServerConfig {
            port: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn validate_http_tls_rejects_missing_and_client_side_fields() {
        let missing = TlsConfig::default();
        assert_eq!(
            validate_http_tls_config(&missing).unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        let client_side = TlsConfig {
            cert_file: Some("cert.pem".to_string()),
            key_file: Some("key.pem".to_string()),
            skip_verify: true,
            min_version: TlsVersion::Tls12,
            ..Default::default()
        };
        let error = validate_http_tls_config(&client_side).unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.to_string().contains("client-side TLS settings"));
    }

    #[test]
    fn http_config_validation_reports_invalid_cors_and_tls_branches() {
        let invalid_cors = HttpServerConfig {
            cors: Some(CorsPolicy {
                allow_credentials: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let errors = invalid_cors.validate().unwrap_err();
        assert!(errors.field_errors().contains_key("cors"));

        let invalid_tls = HttpServerConfig {
            tls: Some(TlsConfig {
                cert_file: Some("cert.pem".to_string()),
                key_file: None,
                min_version: TlsVersion::Tls12,
                ..Default::default()
            }),
            ..Default::default()
        };
        let errors = invalid_tls.validate().unwrap_err();
        assert!(errors.field_errors().contains_key("tls"));

        let valid_tls = HttpServerConfig {
            tls: Some(TlsConfig {
                cert_file: Some("cert.pem".to_string()),
                key_file: Some("key.pem".to_string()),
                min_version: TlsVersion::Tls12,
                ..Default::default()
            }),
            ..Default::default()
        };
        valid_tls.validate().unwrap();
        validate_http_tls_config(valid_tls.tls.as_ref().unwrap()).unwrap();
    }
}

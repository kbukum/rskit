use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_resilience::Policy;
use serde::{Deserialize, Serialize};

/// TLS configuration for gRPC clients.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GrpcTlsConfig {
    /// Override the TLS server name used for certificate verification.
    pub domain_name: Option<String>,
    /// Optional PEM-encoded CA bundle used in addition to enabled roots.
    pub ca_cert_path: Option<String>,
}

/// Configuration for a gRPC client channel.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcClientConfig {
    /// Target address for the gRPC server (for example `localhost:50051`).
    pub target: String,

    /// Optional TLS configuration. When omitted, the channel uses plaintext.
    pub tls: Option<GrpcTlsConfig>,

    /// Default timeout for unary RPCs.
    pub timeout: Duration,

    /// Timeout for establishing a connection.
    pub connect_timeout: Duration,

    /// Keepalive interval (time between pings when no active streams).
    pub keepalive_interval: Option<Duration>,

    /// Keepalive timeout (how long to wait for ping ack before closing).
    pub keepalive_timeout: Option<Duration>,

    /// Maximum message size for receiving (in bytes).
    pub max_message_size: usize,

    /// Maximum message size for sending (in bytes).
    pub max_send_message_size: usize,

    /// Optional transport resilience policy.
    #[serde(skip)]
    pub resilience_policy: Option<Policy>,
}

impl std::fmt::Debug for GrpcClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcClientConfig")
            .field("target", &self.target)
            .field("tls", &self.tls)
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("keepalive_interval", &self.keepalive_interval)
            .field("keepalive_timeout", &self.keepalive_timeout)
            .field("max_message_size", &self.max_message_size)
            .field("max_send_message_size", &self.max_send_message_size)
            .field("has_resilience_policy", &self.resilience_policy.is_some())
            .finish()
    }
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            target: "localhost:50051".to_string(),
            tls: None,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_timeout: Some(Duration::from_secs(10)),
            max_message_size: 4 * 1024 * 1024,
            max_send_message_size: 4 * 1024 * 1024,
            resilience_policy: None,
        }
    }
}

impl GrpcClientConfig {
    /// Create a new [`GrpcClientConfig`] with the given target.
    #[must_use]
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            ..Default::default()
        }
    }

    /// Enable TLS with default modern roots.
    #[must_use]
    pub fn with_tls(mut self, tls: GrpcTlsConfig) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Set the transport resilience policy.
    #[must_use]
    pub fn with_resilience_policy(mut self, policy: Policy) -> Self {
        self.resilience_policy = Some(policy);
        self
    }

    /// Validate the configuration.
    pub fn validate(&self) -> AppResult<()> {
        if self.target.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "grpc client: target must not be empty",
            ));
        }

        if self.max_message_size == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "grpc client: max_message_size must be positive",
            ));
        }

        if self.max_send_message_size == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "grpc client: max_send_message_size must be positive",
            ));
        }

        Ok(())
    }

    /// Get the target address for dialing.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_prefers_plaintext_until_tls_is_configured() {
        let cfg = GrpcClientConfig::default();
        assert_eq!(cfg.target, "localhost:50051");
        assert!(cfg.tls.is_none());
        assert_eq!(cfg.timeout, Duration::from_secs(30));
    }

    #[test]
    fn with_tls_sets_explicit_tls_configuration() {
        let cfg = GrpcClientConfig::new("example.com:443").with_tls(GrpcTlsConfig {
            domain_name: Some("api.example.com".to_string()),
            ca_cert_path: Some("certs/ca.pem".to_string()),
        });
        assert_eq!(
            cfg.tls.as_ref().and_then(|tls| tls.domain_name.as_deref()),
            Some("api.example.com")
        );
        assert_eq!(
            cfg.tls.as_ref().and_then(|tls| tls.ca_cert_path.as_deref()),
            Some("certs/ca.pem")
        );
    }

    #[test]
    fn validate_rejects_empty_target() {
        let cfg = GrpcClientConfig {
            target: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }
}

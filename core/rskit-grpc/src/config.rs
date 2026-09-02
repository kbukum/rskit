use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_resilience::Policy;
use rskit_security::{TlsConfig, TlsVersion};
use serde::{Deserialize, Serialize};

/// Configuration for a gRPC client channel.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcClientConfig {
    /// Target address for the gRPC server (for example `localhost:50051`).
    pub target: String,

    /// Optional shared TLS configuration. When omitted, the channel uses plaintext.
    pub tls: Option<TlsConfig>,

    /// Default timeout for unary RPCs.
    #[serde(with = "rskit_util::time::serde_duration")]
    pub timeout: Duration,

    /// Timeout for establishing a connection.
    #[serde(with = "rskit_util::time::serde_duration")]
    pub connect_timeout: Duration,

    /// Keepalive interval (time between pings when no active streams).
    #[serde(with = "rskit_util::time::serde_duration::option")]
    pub keepalive_interval: Option<Duration>,

    /// Keepalive timeout (how long to wait for ping ack before closing).
    #[serde(with = "rskit_util::time::serde_duration::option")]
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

    /// Enable TLS with shared security configuration and default modern roots.
    #[must_use]
    pub fn with_tls(mut self, tls: TlsConfig) -> Self {
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

        if let Some(tls) = &self.tls {
            validate_grpc_tls(tls)?;
        }

        Ok(())
    }

    /// Get the target address for dialing.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.target
    }
}

pub(crate) fn validate_grpc_tls(tls: &TlsConfig) -> AppResult<()> {
    tls.validate()?;
    if tls.skip_verify {
        return Err(AppError::invalid_input(
            "tls.skip_verify",
            "gRPC client TLS does not allow disabling peer verification",
        ));
    }
    if tls.min_version != TlsVersion::Tls12 {
        return Err(AppError::invalid_input(
            "tls.min_version",
            "tonic ClientTlsConfig does not expose a minimum protocol-version floor; gRPC clients currently support the rustls default TLS 1.2 floor",
        ));
    }
    Ok(())
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
        assert_eq!(cfg.address(), "localhost:50051");
        assert!(
            format!("{:?}", cfg.with_resilience_policy(Policy::new()))
                .contains("has_resilience_policy")
        );
    }

    #[test]
    fn durations_deserialize_from_strings_and_round_trip_losslessly() {
        let cfg: GrpcClientConfig = serde_json::from_str(
            r#"{
                "target":"api:50051",
                "timeout":"45s",
                "connect_timeout":"1.5s",
                "keepalive_interval":"20s",
                "keepalive_timeout":null
            }"#,
        )
        .expect("string durations load");
        assert_eq!(cfg.timeout, Duration::from_secs(45));
        assert_eq!(cfg.connect_timeout, Duration::from_millis(1500));
        assert_eq!(cfg.keepalive_interval, Some(Duration::from_secs(20)));
        assert_eq!(cfg.keepalive_timeout, None);

        let json = serde_json::to_value(&cfg).expect("serialize");
        assert_eq!(json["timeout"], "45s");
        assert_eq!(json["connect_timeout"], "1500ms");
        assert_eq!(json["keepalive_interval"], "20s");
        assert!(json["keepalive_timeout"].is_null());
    }

    #[test]
    fn with_tls_sets_explicit_tls_configuration() {
        let cfg = GrpcClientConfig::new("example.com:443").with_tls(TlsConfig {
            server_name: Some("api.example.com".to_string()),
            ca_file: Some("certs/ca.pem".to_string()),
            cert_file: Some("certs/client.pem".to_string()),
            key_file: Some("certs/client.key".to_string()),
            ..Default::default()
        });
        assert_eq!(
            cfg.tls.as_ref().and_then(|tls| tls.server_name.as_deref()),
            Some("api.example.com")
        );
        assert_eq!(
            cfg.tls.as_ref().and_then(|tls| tls.ca_file.as_deref()),
            Some("certs/ca.pem")
        );
    }

    #[test]
    fn validate_rejects_unsupported_insecure_tls_options() {
        let cfg = GrpcClientConfig::new("example.com:443").with_tls(TlsConfig {
            skip_verify: true,
            ..Default::default()
        });

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_incomplete_client_identity() {
        let cfg = GrpcClientConfig::new("example.com:443").with_tls(TlsConfig {
            cert_file: Some("client.pem".to_string()),
            ..Default::default()
        });

        assert_eq!(cfg.validate().unwrap_err().code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn validate_rejects_unsupported_tls13_floor() {
        let cfg = GrpcClientConfig::new("example.com:443").with_tls(TlsConfig {
            min_version: rskit_security::TlsVersion::Tls13,
            ..Default::default()
        });

        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_target() {
        let cfg = GrpcClientConfig {
            target: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_message_limits_independently() {
        let recv_zero = GrpcClientConfig {
            max_message_size: 0,
            ..Default::default()
        };
        assert!(
            recv_zero
                .validate()
                .unwrap_err()
                .to_string()
                .contains("max_message_size")
        );

        let send_zero = GrpcClientConfig {
            max_send_message_size: 0,
            ..Default::default()
        };
        assert!(
            send_zero
                .validate()
                .unwrap_err()
                .to_string()
                .contains("max_send_message_size")
        );
    }

    #[test]
    fn debug_output_reports_resilience_policy_presence_without_dumping_policy() {
        let cfg = GrpcClientConfig::new("example.com:443").with_resilience_policy(Policy::new());

        let rendered = format!("{cfg:?}");

        assert!(rendered.contains("example.com:443"));
        assert!(rendered.contains("has_resilience_policy: true"));
    }

    #[test]
    fn serde_defaults_fill_optional_transport_settings() {
        let cfg: GrpcClientConfig = serde_json::from_value(serde_json::json!({
            "target": "api.internal:443",
            "max_message_size": 1024,
            "max_send_message_size": 2048
        }))
        .unwrap();

        assert_eq!(cfg.address(), "api.internal:443");
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
        assert_eq!(cfg.keepalive_interval, Some(Duration::from_secs(30)));
        assert_eq!(cfg.max_message_size, 1024);
        assert_eq!(cfg.max_send_message_size, 2048);
    }
}

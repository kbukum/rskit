use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for a gRPC client channel.
///
/// Mirrors [`gokit/grpc.Config`] and [`pykit_grpc.GrpcConfig`] with Rust-appropriate
/// defaults and patterns.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcClientConfig {
    /// Target address for the gRPC server (e.g., "localhost:50051").
    pub target: String,

    /// Whether to use TLS. If false, uses insecure connection.
    pub tls: bool,

    /// Default timeout for unary RPCs.
    pub timeout: Duration,

    /// Timeout for establishing a connection.
    pub connect_timeout: Duration,

    /// Keepalive interval (time between pings when no active streams).
    /// If None, keepalive is disabled.
    pub keepalive_interval: Option<Duration>,

    /// Keepalive timeout (how long to wait for ping ack before closing).
    /// If None, no timeout is set.
    pub keepalive_timeout: Option<Duration>,

    /// Maximum message size for receiving (in bytes).
    pub max_message_size: usize,

    /// Maximum message size for sending (in bytes).
    pub max_send_message_size: usize,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            target: "localhost:50051".to_string(),
            tls: false,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_timeout: Some(Duration::from_secs(10)),
            max_message_size: 4 * 1024 * 1024, // 4 MB
            max_send_message_size: 4 * 1024 * 1024, // 4 MB
        }
    }
}

impl GrpcClientConfig {
    /// Create a new [`GrpcClientConfig`] with the given target.
    ///
    /// All other fields are initialized to their defaults.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            ..Default::default()
        }
    }

    /// Apply defaults to any unspecified fields.
    ///
    /// This is a no-op for this struct since `Default` is already applied,
    /// but provided for API consistency with gokit.
    pub fn apply_defaults(&mut self) {
        // No-op: Default trait already handles this
    }

    /// Validate the configuration.
    pub fn validate(&self) -> rskit_errors::AppResult<()> {
        if self.target.is_empty() {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "grpc client: target must not be empty",
            ));
        }

        if self.max_message_size == 0 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "grpc client: max_message_size must be positive",
            ));
        }

        if self.max_send_message_size == 0 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "grpc client: max_send_message_size must be positive",
            ));
        }

        Ok(())
    }

    /// Get the target address for dial.
    pub fn address(&self) -> &str {
        &self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = GrpcClientConfig::default();
        assert_eq!(cfg.target, "localhost:50051");
        assert!(!cfg.tls);
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
        assert_eq!(cfg.keepalive_interval, Some(Duration::from_secs(30)));
        assert_eq!(cfg.keepalive_timeout, Some(Duration::from_secs(10)));
        assert_eq!(cfg.max_message_size, 4 * 1024 * 1024);
        assert_eq!(cfg.max_send_message_size, 4 * 1024 * 1024);
    }

    #[test]
    fn test_new_config() {
        let cfg = GrpcClientConfig::new("example.com:9090");
        assert_eq!(cfg.target, "example.com:9090");
        assert!(!cfg.tls);
    }

    #[test]
    fn test_validate_success() {
        let cfg = GrpcClientConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_target() {
        let cfg = GrpcClientConfig {
            target: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_zero_message_size() {
        let cfg = GrpcClientConfig {
            max_message_size: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }
}

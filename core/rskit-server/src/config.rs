use serde::{Deserialize, Serialize};
use rskit_validation::Validate;

/// TLS configuration for the gRPC server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    /// Path to the PEM-encoded TLS certificate file.
    pub cert_path: String,
    /// Path to the PEM-encoded TLS private key file.
    pub key_path: String,
}

/// Configuration for the gRPC server component.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct GrpcServerConfig {
    /// Bind address (e.g. `"0.0.0.0"` or `"127.0.0.1"`).
    #[validate(length(min = 1))]
    pub host: String,

    /// TCP port to listen on (1–65535).
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Optional maximum number of concurrent connections.
    pub max_connections: Option<usize>,

    /// Optional TCP keep-alive interval in seconds.
    pub keep_alive_secs: Option<u64>,

    /// Optional TLS configuration.
    ///
    /// When configured, tonic/rustls modern defaults are used with TLS 1.3
    /// preferred and TLS 1.2 as the minimum supported version.
    pub tls: Option<TlsConfig>,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 50051,
            max_connections: None,
            keep_alive_secs: None,
            tls: None,
        }
    }
}

impl GrpcServerConfig {
    /// Create a config with the given `host` and `port`.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Default::default()
        }
    }

    /// Returns the `host:port` socket address string.
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_validation::Validate;

    #[test]
    fn default_config_is_valid() {
        let cfg = GrpcServerConfig::default();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 50051);
    }

    #[test]
    fn new_sets_host_and_port() {
        let cfg = GrpcServerConfig::new("127.0.0.1", 8080);
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8080);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn addr_formats_correctly() {
        let cfg = GrpcServerConfig::new("localhost", 9090);
        assert_eq!(cfg.addr(), "localhost:9090");
    }

    #[test]
    fn empty_host_fails_validation() {
        let cfg = GrpcServerConfig {
            host: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn tls_config_stores_paths() {
        let tls = TlsConfig {
            cert_path: "/certs/server.crt".into(),
            key_path: "/certs/server.key".into(),
        };
        let cfg = GrpcServerConfig {
            tls: Some(tls.clone()),
            ..Default::default()
        };
        let stored = cfg.tls.unwrap();
        assert_eq!(stored.cert_path, "/certs/server.crt");
        assert_eq!(stored.key_path, "/certs/server.key");
    }

    #[test]
    fn optional_fields_default_to_none() {
        let cfg = GrpcServerConfig::default();
        assert!(cfg.max_connections.is_none());
        assert!(cfg.keep_alive_secs.is_none());
        assert!(cfg.tls.is_none());
    }
}

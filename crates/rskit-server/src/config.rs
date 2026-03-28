use serde::{Deserialize, Serialize};
use validator::Validate;

/// TLS configuration for the gRPC server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

/// Configuration for the gRPC server component.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct GrpcServerConfig {
    #[validate(length(min = 1))]
    pub host: String,

    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    pub max_connections: Option<usize>,

    pub keep_alive_secs: Option<u64>,

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
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self { host: host.into(), port, ..Default::default() }
    }

    /// Returns the `host:port` socket address string.
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

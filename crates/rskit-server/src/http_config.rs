use std::time::Duration;

use serde::Deserialize;
use validator::Validate;

/// HTTP server configuration owned by `rskit-server`.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct HttpServerConfig {
    /// Bind address (default: `0.0.0.0`).
    #[serde(default = "HttpServerConfig::default_host")]
    pub host: String,

    /// Bind port (default: `8080`).
    #[serde(default = "HttpServerConfig::default_port")]
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Maximum time to wait for a request header (default: 30 s).
    #[serde(default = "HttpServerConfig::default_timeout")]
    pub read_timeout: Duration,

    /// Maximum time to wait before sending the first response byte (default: 30 s).
    #[serde(default = "HttpServerConfig::default_timeout")]
    pub write_timeout: Duration,

    /// Idle keep-alive connection timeout (default: 60 s).
    #[serde(default = "HttpServerConfig::default_idle_timeout")]
    pub idle_timeout: Duration,

    /// Enable HTTP/2 cleartext (h2c) on the same port (default: `true`).
    #[serde(default = "HttpServerConfig::default_h2c")]
    pub enable_h2c: bool,

    /// Optional CORS policy.
    pub cors: Option<CorsConfig>,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: Self::default_host(),
            port: Self::default_port(),
            read_timeout: Self::default_timeout(),
            write_timeout: Self::default_timeout(),
            idle_timeout: Self::default_idle_timeout(),
            enable_h2c: Self::default_h2c(),
            cors: None,
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

/// Cross-origin resource sharing configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins.
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,
    /// Allowed request headers.
    pub allowed_headers: Vec<String>,
    /// Whether to allow sending credentials (cookies, auth headers).
    pub allow_credentials: bool,
    /// Cache duration for pre-flight responses.
    pub max_age: Duration,
}

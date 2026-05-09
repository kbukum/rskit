//! MCP transport naming and Streamable HTTP configuration helpers.

use rmcp::transport::StreamableHttpServerConfig;

/// Canonical MCP stdio transport name.
pub const TRANSPORT_STDIO: &str = "stdio";

/// Canonical MCP Streamable HTTP transport name.
pub const TRANSPORT_STREAMABLE_HTTP: &str = "streamable_http";

/// Canonical MCP transport kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// Stdio transport.
    Stdio,
    /// Streamable HTTP transport.
    StreamableHttp,
}

impl TransportKind {
    /// Return the canonical transport name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => TRANSPORT_STDIO,
            Self::StreamableHttp => TRANSPORT_STREAMABLE_HTTP,
        }
    }
}

impl TryFrom<&str> for TransportKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            TRANSPORT_STDIO => Ok(Self::Stdio),
            TRANSPORT_STREAMABLE_HTTP => Ok(Self::StreamableHttp),
            _ => Err(format!(
                "unsupported MCP transport {value:?}: use {TRANSPORT_STDIO:?} or {TRANSPORT_STREAMABLE_HTTP:?}"
            )),
        }
    }
}

/// Build a Streamable HTTP server config with loopback-safe defaults.
pub fn streamable_http_server_config(
    allowed_hosts: impl IntoIterator<Item = impl Into<String>>,
    allowed_origins: impl IntoIterator<Item = impl Into<String>>,
) -> Result<StreamableHttpServerConfig, String> {
    let hosts: Vec<String> = allowed_hosts.into_iter().map(Into::into).collect();
    let origins: Vec<String> = allowed_origins.into_iter().map(Into::into).collect();
    for origin in &origins {
        validate_allowed_origin(origin)?;
    }

    let mut config = StreamableHttpServerConfig::default();
    if !hosts.is_empty() {
        config = config.with_allowed_hosts(hosts);
    }
    if !origins.is_empty() {
        config = config.with_allowed_origins(origins);
    }
    Ok(config)
}

fn validate_allowed_origin(origin: &str) -> Result<(), String> {
    let parsed =
        url::Url::parse(origin).map_err(|e| format!("invalid allowed origin {origin:?}: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        s => {
            return Err(format!(
                "invalid allowed origin {origin:?}: scheme must be http or https, got {s:?}"
            ));
        }
    }
    if parsed.host_str().unwrap_or("").is_empty() {
        return Err(format!("invalid allowed origin {origin:?}: missing host"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!(
            "invalid allowed origin {origin:?}: origin must not contain credentials"
        ));
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err(format!(
            "invalid allowed origin {origin:?}: origin must not contain a path"
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(format!(
            "invalid allowed origin {origin:?}: origin must not contain query or fragment"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TRANSPORT_STDIO, TRANSPORT_STREAMABLE_HTTP, TransportKind, streamable_http_server_config,
    };

    #[test]
    fn parses_transport_names() {
        assert_eq!(
            TransportKind::try_from(TRANSPORT_STDIO).unwrap(),
            TransportKind::Stdio
        );
        assert_eq!(
            TransportKind::try_from(TRANSPORT_STREAMABLE_HTTP).unwrap(),
            TransportKind::StreamableHttp
        );
        assert!(TransportKind::try_from("sse").is_err());
    }

    #[test]
    fn builds_streamable_http_server_config() {
        let config =
            streamable_http_server_config(["localhost", "127.0.0.1"], ["https://app.example.com"])
                .unwrap();
        assert_eq!(config.allowed_hosts, vec!["localhost", "127.0.0.1"]);
        assert_eq!(config.allowed_origins, vec!["https://app.example.com"]);
    }

    #[test]
    fn rejects_invalid_allowed_origins() {
        let error = streamable_http_server_config(["localhost"], ["localhost:3000"]).unwrap_err();
        assert!(error.contains("invalid allowed origin"));
    }

    #[test]
    fn rejects_origin_with_path() {
        let error =
            streamable_http_server_config(["localhost"], ["https://example.com/path"]).unwrap_err();
        assert!(error.contains("must not contain a path"));
    }

    #[test]
    fn rejects_non_http_scheme() {
        let error =
            streamable_http_server_config(["localhost"], ["javascript://alert"]).unwrap_err();
        assert!(error.contains("scheme must be http or https"));
    }
}

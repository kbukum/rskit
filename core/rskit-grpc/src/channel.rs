use std::path::Path;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::async_io::file;
use rskit_security::TlsConfig;
use tokio::sync::RwLock;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tracing::{debug, warn};

use crate::config::{GrpcClientConfig, validate_grpc_tls};

/// Lazy-connecting gRPC channel wrapper.
pub struct GrpcChannel {
    config: GrpcClientConfig,
    channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcChannel {
    /// Create a new [`GrpcChannel`] with the given configuration.
    #[must_use]
    pub fn new(config: GrpcClientConfig) -> Self {
        debug!(target = %config.target, "creating gRPC channel");
        Self {
            config,
            channel: Arc::new(RwLock::new(None)),
        }
    }

    /// Establish a connection to the gRPC server.
    pub async fn connect(&self) -> AppResult<()> {
        {
            let channel = self.channel.read().await;
            if channel.is_some() {
                return Ok(());
            }
        }

        let config = self.config.clone();
        let connect =
            || async {
                let endpoint = build_endpoint(&config).await?;
                endpoint.connect().await.map_err(|error| {
                warn!(target = %config.target, error = %error, "failed to connect gRPC channel");
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("failed to connect gRPC channel to {}: {}", config.target, error),
                )
                .retryable(true)
                .with_cause(error)
            })
            };

        let channel = if let Some(policy) = &self.config.resilience_policy {
            policy.execute(connect).await?
        } else {
            connect().await?
        };

        *self.channel.write().await = Some(channel);
        Ok(())
    }

    /// Get a connected channel, establishing the connection when needed.
    pub async fn connected_channel(&self) -> AppResult<Channel> {
        {
            let channel = self.channel.read().await;
            if let Some(channel) = channel.as_ref() {
                return Ok(channel.clone());
            }
        }

        self.connect().await?;

        self.channel.read().await.clone().ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                "channel disappeared after a successful connection",
            )
        })
    }

    /// Alias for [`connected_channel`](Self::connected_channel).
    pub async fn channel(&self) -> AppResult<Channel> {
        self.connected_channel().await
    }

    /// Check whether the channel is ready for RPCs.
    pub async fn is_ready(&self) -> bool {
        self.connected_channel().await.is_ok()
    }

    /// Close the channel gracefully.
    pub async fn close(&mut self) -> AppResult<()> {
        *self.channel.write().await = None;
        Ok(())
    }

    /// Get the target address.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.config.target
    }

    /// Get the configuration.
    #[must_use]
    pub fn config(&self) -> &GrpcClientConfig {
        &self.config
    }
}

impl Clone for GrpcChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            channel: Arc::clone(&self.channel),
        }
    }
}

async fn build_endpoint(config: &GrpcClientConfig) -> AppResult<Endpoint> {
    let scheme = if config.tls.is_some() {
        "https"
    } else {
        "http"
    };
    let mut endpoint =
        Endpoint::from_shared(format!("{scheme}://{}", config.target)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid gRPC endpoint '{}': {}", config.target, error),
            )
            .with_cause(error)
        })?;

    endpoint = endpoint.timeout(config.timeout);
    endpoint = endpoint.connect_timeout(config.connect_timeout);

    if let Some(interval) = config.keepalive_interval
        && let Some(timeout) = config.keepalive_timeout
    {
        endpoint = endpoint.keep_alive_while_idle(true);
        endpoint = endpoint.http2_keep_alive_interval(interval);
        endpoint = endpoint.keep_alive_timeout(timeout);
    }

    if let Some(tls) = &config.tls {
        endpoint = endpoint
            .tls_config(build_tls_config(config, tls).await?)
            .map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("invalid gRPC TLS configuration: {error}"),
                )
                .with_cause(error)
            })?;
    }

    Ok(endpoint)
}

async fn build_tls_config(
    config: &GrpcClientConfig,
    tls: &TlsConfig,
) -> AppResult<ClientTlsConfig> {
    validate_grpc_tls(tls)?;
    let mut tls_config = ClientTlsConfig::new()
        .with_enabled_roots()
        .domain_name(default_tls_domain_name(config, tls));

    if let Some(ca_file) = &tls.ca_file {
        let pem = file::read(Path::new(ca_file)).await.map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read gRPC CA bundle '{}': {}", ca_file, error),
            )
            .with_cause(error)
        })?;
        tls_config = tls_config.ca_certificate(Certificate::from_pem(pem));
    }

    if let (Some(cert_file), Some(key_file)) = (&tls.cert_file, &tls.key_file) {
        let cert = file::read(Path::new(cert_file)).await.map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "failed to read gRPC client certificate '{}': {}",
                    cert_file, error
                ),
            )
            .with_cause(error)
        })?;
        let key = file::read(Path::new(key_file)).await.map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read gRPC client key '{}': {}", key_file, error),
            )
            .with_cause(error)
        })?;
        tls_config = tls_config.identity(Identity::from_pem(cert, key));
    }

    Ok(tls_config)
}

fn default_tls_domain_name(config: &GrpcClientConfig, tls: &TlsConfig) -> String {
    if let Some(domain_name) = tls.server_name.as_ref().filter(|value| !value.is_empty()) {
        return domain_name.clone();
    }

    format!("http://{}", config.target)
        .parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.host().map(str::to_owned))
        .map(|host| host.trim_matches(['[', ']']).to_string())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "localhost".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_channel_keeps_target() {
        let channel = GrpcChannel::new(GrpcClientConfig::new("localhost:50051"));
        assert_eq!(channel.target(), "localhost:50051");
    }

    #[tokio::test]
    #[ignore = "requires native TLS certificates; run with --include-ignored in a configured environment"]
    async fn build_endpoint_uses_https_when_tls_is_enabled() {
        let ca_cert_path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/ca.pem").to_string();
        let endpoint = build_endpoint(&GrpcClientConfig::new("example.com:443").with_tls(
            TlsConfig {
                ca_file: Some(ca_cert_path),
                ..Default::default()
            },
        ))
        .await
        .unwrap();
        assert_eq!(endpoint.uri().scheme_str(), Some("https"));
    }

    #[test]
    fn default_tls_domain_name_uses_ipv6_host_without_brackets() {
        let tls = TlsConfig::default();
        let config = GrpcClientConfig::new("[::1]:50051").with_tls(tls.clone());
        assert_eq!(default_tls_domain_name(&config, &tls), "::1");
    }

    #[test]
    fn default_tls_domain_name_prefers_explicit_domain() {
        let tls = TlsConfig {
            server_name: Some("grpc.internal".to_string()),
            ..Default::default()
        };
        let config = GrpcClientConfig::new("127.0.0.1:50051").with_tls(tls.clone());
        assert_eq!(default_tls_domain_name(&config, &tls), "grpc.internal");
    }

    #[test]
    fn default_tls_domain_name_falls_back_to_localhost_for_invalid_targets() {
        let tls = TlsConfig::default();
        let config = GrpcClientConfig::new("not a uri").with_tls(tls.clone());

        assert_eq!(default_tls_domain_name(&config, &tls), "localhost");
    }

    #[tokio::test]
    async fn build_endpoint_rejects_invalid_plaintext_targets_before_connecting() {
        let err = build_endpoint(&GrpcClientConfig::new("not a uri"))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("invalid gRPC endpoint"));
    }

    #[tokio::test]
    async fn build_endpoint_applies_tls_and_keepalive_without_connecting() {
        let endpoint = build_endpoint(
            &GrpcClientConfig::new("example.com:443").with_tls(TlsConfig::default()),
        )
        .await
        .unwrap();

        assert_eq!(endpoint.uri().scheme_str(), Some("https"));
    }

    #[tokio::test]
    async fn connect_to_closed_local_port_returns_retryable_service_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);
        let channel = GrpcChannel::new(GrpcClientConfig::new(addr.to_string()));

        let err = channel.connect().await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
        assert!(err.is_retryable());
    }

    #[tokio::test]
    async fn build_tls_config_reports_missing_ca_bundle() {
        let tls = TlsConfig {
            ca_file: Some("missing/ca.pem".to_string()),
            ..Default::default()
        };
        let config = GrpcClientConfig::new("example.com:443").with_tls(tls.clone());

        let err = build_tls_config(&config, &tls).await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("failed to read gRPC CA bundle"));
    }

    #[tokio::test]
    async fn close_clears_channel_without_requiring_connection() {
        let mut channel = GrpcChannel::new(GrpcClientConfig::new("localhost:50051"));

        channel.close().await.unwrap();

        assert_eq!(channel.target(), "localhost:50051");
        assert_eq!(channel.config().address(), "localhost:50051");
    }

    #[tokio::test]
    async fn connected_channel_uses_local_listener_and_caches_result() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _addr) = listener.accept().await.expect("accept connection");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        });
        let channel = GrpcChannel::new(
            GrpcClientConfig::new(addr.to_string())
                .with_resilience_policy(rskit_resilience::Policy::new()),
        );

        channel.connect().await.expect("connect to local listener");
        let _first = channel.connected_channel().await.expect("cached channel");
        let _second = channel
            .channel()
            .await
            .expect("alias returns cached channel");

        assert_eq!(channel.target(), addr.to_string());
        assert!(channel.is_ready().await);
        accept_task.abort();
    }

    #[tokio::test]
    async fn build_tls_config_reports_missing_client_key_after_reading_cert() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("rskit-grpc-tests")
            .join(format!("tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        std::fs::create_dir_all(&workspace).expect("create workspace");
        let cert = workspace.join("client.pem");
        std::fs::write(&cert, b"not a real cert").expect("write cert");
        let missing_key = workspace.join("missing.key");
        let tls = TlsConfig {
            cert_file: Some(cert.display().to_string()),
            key_file: Some(missing_key.display().to_string()),
            ..Default::default()
        };
        let config = GrpcClientConfig::new("example.com:443").with_tls(tls.clone());

        let err = build_tls_config(&config, &tls).await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("failed to read gRPC client key"));
        let _ = std::fs::remove_dir_all(&workspace);
    }
}

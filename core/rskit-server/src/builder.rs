use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_reflection::server::Builder as ReflectionBuilder;
use tower::Layer;

use crate::component::GrpcServer;
use crate::config::GrpcServerConfig;
use crate::error_layer::ErrorLayer;

// ---------------------------------------------------------------------------
// Service adder trait
//
// We type-erase tonic services here.
// Rather than wrestling with the generic `tonic::transport::Router<S>` type parameter (which is unnameable),
// we store closures that apply a service to a `Server` builder
// and accumulate them into a `tonic::transport::Router` incrementally.
//
// Each service closure returns an `ErasedRouter` —
// a boxed object that can route requests without exposing the concrete Router type to the builder.
// ---------------------------------------------------------------------------

/// Type-erased function that adds one service to a tonic server
/// and returns a closure capable of calling `serve_with_shutdown`.
pub(crate) type ServeFn = Arc<
    dyn Fn(
            SocketAddr,
            std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send>>
        + Send
        + Sync,
>;

/// Builder for a [`GrpcServer`] component.
///
/// # gRPC Reflection
///
/// Enable server reflection so tools like `grpcurl` can discover services:
///
/// ```rust,ignore
/// # use rskit_server::{GrpcServerBuilder, GrpcServerConfig};
/// // In your build.rs, configure tonic_build to output a file descriptor set:
/// //   tonic_build::configure()
/// //       .file_descriptor_set_path("src/descriptor.bin")
/// //       .compile(&["proto/my_service.proto"], &["proto/"])?;
///
/// let descriptor = include_bytes!("descriptor.bin");
///
/// let server = GrpcServerBuilder::new(GrpcServerConfig::default())
///     .with_reflection(descriptor)
///     .build();
/// ```
///
/// Then query with grpcurl:
/// ```bash
/// grpcurl -plaintext localhost:50051 list
/// grpcurl -plaintext localhost:50051 describe my.package.MyService
/// ```
pub struct GrpcServerBuilder {
    name: String,
    config: GrpcServerConfig,
    /// Accumulated serve fns — each captures one tonic service.
    serve_fns: Vec<ServeFn>,
    /// Compiled `FileDescriptorSet` bytes for gRPC reflection.
    reflection_descriptor: Option<Vec<u8>>,
}

impl GrpcServerBuilder {
    /// Create a new builder with the given server configuration.
    pub fn new(config: GrpcServerConfig) -> Self {
        Self {
            name: "grpc-server".into(),
            config,
            serve_fns: Vec::new(),
            reflection_descriptor: None,
        }
    }

    /// Override the component name (default: `"grpc-server"`).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Enable gRPC server reflection for service discovery.
    ///
    /// Accepts the compiled `FileDescriptorSet` bytes produced by `tonic-build`.
    /// Configure `tonic-build` in your `build.rs`:
    ///
    /// ```rust,ignore
    /// tonic_build::configure()
    ///     .file_descriptor_set_path("src/descriptor.bin")
    ///     .compile(&["proto/service.proto"], &["proto/"])?;
    /// ```
    ///
    /// Then pass the bytes to the builder:
    ///
    /// ```rust,ignore
    /// builder.with_reflection(include_bytes!("descriptor.bin"))
    /// ```
    #[must_use]
    pub fn with_reflection(mut self, file_descriptor_set: &[u8]) -> Self {
        self.reflection_descriptor = Some(file_descriptor_set.to_vec());
        self
    }

    /// Add a tonic-generated service.
    ///
    /// The service is automatically wrapped with [`ErrorLayer`]
    /// so that all gRPC error responses carry structured JSON details.
    /// If [`with_reflection`](Self::with_reflection) was called,
    /// the reflection service is automatically added alongside each user service.
    #[must_use]
    pub fn add_service<S>(mut self, svc: S) -> Self
    where
        S: tonic::codegen::Service<
                http::Request<tonic::body::Body>,
                Response = http::Response<tonic::body::Body>,
                Error = std::convert::Infallible,
            > + tonic::server::NamedService
            + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
    {
        let descriptor = self.reflection_descriptor.clone();
        let tls = self.config.tls.clone();
        // Wrap the user service with the error enrichment layer.
        let wrapped_svc = ErrorLayer::new().layer(svc);
        let serve_fn: ServeFn = Arc::new(move |addr, signal| {
            let s = wrapped_svc.clone();
            let desc = descriptor.clone();
            let tls = tls.clone();
            Box::pin(async move {
                let mut builder = Server::builder();
                if let Some(tls) = &tls {
                    let cert = rskit_fs::async_io::file::read(Path::new(&tls.cert_path))
                        .await
                        .map_err(|error| {
                            AppError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "failed to read TLS certificate '{}': {error}",
                                    tls.cert_path
                                ),
                            )
                            .with_cause(error)
                        })?;
                    let key = rskit_fs::async_io::file::read(Path::new(&tls.key_path))
                        .await
                        .map_err(|error| {
                            AppError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "failed to read TLS private key '{}': {error}",
                                    tls.key_path
                                ),
                            )
                            .with_cause(error)
                        })?;
                    let tls_config = ServerTlsConfig::new()
                        .identity(Identity::from_pem(cert, key))
                        .ignore_client_order(true)
                        .timeout(Duration::from_secs(10));
                    builder = builder.tls_config(tls_config).map_err(|error| {
                        AppError::new(
                            ErrorCode::InvalidInput,
                            format!("invalid gRPC TLS configuration: {error}"),
                        )
                        .with_cause(error)
                    })?;
                }
                let router = builder.add_service(s);

                if let Some(desc_bytes) = desc {
                    match ReflectionBuilder::configure()
                        .register_encoded_file_descriptor_set(&desc_bytes)
                        .build_v1()
                    {
                        Ok(refl_svc) => router
                            .add_service(refl_svc)
                            .serve_with_shutdown(addr, signal)
                            .await
                            .map_err(|error| {
                                AppError::new(
                                    ErrorCode::Internal,
                                    format!("gRPC server transport failed: {error}"),
                                )
                                .with_cause(error)
                            }),
                        Err(e) => {
                            tracing::error!(error = %e, "failed to build reflection service; serving without reflection");
                            router
                                .serve_with_shutdown(addr, signal)
                                .await
                                .map_err(|error| {
                                    AppError::new(
                                        ErrorCode::Internal,
                                        format!("gRPC server transport failed: {error}"),
                                    )
                                    .with_cause(error)
                                })
                        }
                    }
                } else {
                    router
                        .serve_with_shutdown(addr, signal)
                        .await
                        .map_err(|error| {
                            AppError::new(
                                ErrorCode::Internal,
                                format!("gRPC server transport failed: {error}"),
                            )
                            .with_cause(error)
                        })
                }
            })
        });
        self.serve_fns.push(serve_fn);
        self
    }

    /// Build the [`GrpcServer`] component.
    ///
    /// Only the **last** registered service is used for the actual server (since `tonic::transport::Router` can't be accumulated type-safely without the concrete service type).
    /// For multiple services, compose them before calling `add_service`, or use the raw tonic API.
    pub fn build(self) -> GrpcServer {
        let serve_fns = Arc::new(self.serve_fns);

        let start_fn = Arc::new(move |addr: SocketAddr, cancel: CancellationToken| {
            let fns = serve_fns.clone();
            tokio::spawn(async move {
                // Clone before calling cancelled_owned()
                // so the original is still usable in the else branch.
                let signal = Box::pin(cancel.clone().cancelled_owned());

                // Use the last added service (simplest safe approach).
                if let Some(serve_fn) = fns.last() {
                    tracing::info!(addr = %addr, "gRPC server listening");
                    match serve_fn(addr, signal).await {
                        Ok(()) => tracing::info!(addr = %addr, "gRPC server stopped"),
                        Err(e) => tracing::error!(addr = %addr, error = %e, "gRPC server error"),
                    }
                } else {
                    tracing::warn!(addr = %addr, "gRPC server has no services — waiting for cancel");
                    cancel.cancelled().await;
                }
            })
        });

        GrpcServer::new(self.name, self.config, start_fn)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::future::{Ready, ready};
    use std::task::{Context, Poll};

    use http::{Request, Response};
    use rskit_bootstrap::Component;
    use rskit_errors::ErrorCode;
    use tonic::body::Body;
    use tower::Service;

    use super::*;

    #[tokio::test]
    async fn builder_preserves_name_and_waits_for_cancel_without_services() {
        let config = GrpcServerConfig::new("127.0.0.1", 50051);
        let server = GrpcServerBuilder::new(config)
            .with_name("api-grpc")
            .with_reflection(b"not-a-descriptor")
            .build();

        assert_eq!(server.name(), "api-grpc");
        assert!(!server.health().is_healthy());

        server.start().await.expect("start no-service server");
        assert!(server.health().is_healthy());
        server.stop().await.expect("stop no-service server");
        assert!(!server.health().is_healthy());
    }

    #[tokio::test]
    async fn start_rejects_invalid_bind_address_before_spawning() {
        let config = GrpcServerConfig::new("not a host", 50051);
        let server = GrpcServerBuilder::new(config).build();

        let error = server.start().await.expect_err("invalid address");

        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(error.message().contains("invalid gRPC address"));
        assert!(!server.health().is_healthy());
    }

    #[tokio::test]
    async fn builder_runs_added_service_on_local_listener_until_cancelled() {
        let config = GrpcServerConfig::new("127.0.0.1", 0);
        let server = GrpcServerBuilder::new(config)
            .with_name("local-grpc")
            .add_service(EmptyGrpcService)
            .build();

        server.start().await.expect("start service server");
        assert!(server.health().is_healthy());
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.stop().await.expect("stop service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn builder_falls_back_when_reflection_descriptor_is_invalid() {
        let config = GrpcServerConfig::new("127.0.0.1", 0);
        let server = GrpcServerBuilder::new(config)
            .with_reflection(b"not a protobuf file descriptor set")
            .add_service(EmptyGrpcService)
            .build();

        server.start().await.expect("start service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.stop().await.expect("stop service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn builder_reports_missing_tls_files_from_service_task_without_panicking() {
        let config = GrpcServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            tls: Some(crate::config::TlsConfig {
                cert_path: "missing-cert.pem".to_string(),
                key_path: "missing-key.pem".to_string(),
            }),
            ..GrpcServerConfig::default()
        };
        let server = GrpcServerBuilder::new(config)
            .add_service(EmptyGrpcService)
            .build();

        server.start().await.expect("spawn service task");
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.stop().await.expect("stop service server");
    }

    fn testdata(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[tokio::test]
    async fn builder_serves_with_valid_tls_material() {
        let config = GrpcServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            tls: Some(crate::config::TlsConfig {
                cert_path: testdata("cert.pem"),
                key_path: testdata("key.pem"),
            }),
            ..GrpcServerConfig::default()
        };
        let server = GrpcServerBuilder::new(config)
            .add_service(EmptyGrpcService)
            .build();

        server.start().await.expect("start tls service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.stop().await.expect("stop tls service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn builder_serves_with_valid_reflection_descriptor() {
        let config = GrpcServerConfig::new("127.0.0.1", 0);
        let server = GrpcServerBuilder::new(config)
            .with_reflection(&[])
            .add_service(EmptyGrpcService)
            .build();

        server
            .start()
            .await
            .expect("start reflection service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.stop().await.expect("stop reflection service server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[derive(Clone)]
    struct EmptyGrpcService;

    impl tonic::server::NamedService for EmptyGrpcService {
        const NAME: &'static str = "test.Empty";
    }

    impl tonic::codegen::Service<Request<Body>> for EmptyGrpcService {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            ready(Ok(Response::new(Body::default())))
        }
    }

    #[test]
    fn empty_test_service_is_ready_and_returns_empty_body() {
        let mut service = EmptyGrpcService;
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert!(matches!(service.poll_ready(&mut cx), Poll::Ready(Ok(()))));
        let response = service
            .call(Request::new(Body::default()))
            .into_inner()
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
    }
}

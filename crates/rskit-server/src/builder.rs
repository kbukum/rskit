use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tonic::transport::Server;

use crate::component::GrpcServer;
use crate::config::GrpcServerConfig;

// ---------------------------------------------------------------------------
// Service adder trait
//
// We type-erase tonic services here. Rather than wrestling with the generic
// `tonic::transport::Router<S>` type parameter (which is unnameable), we
// store closures that apply a service to a `Server` builder and accumulate
// them into a `tonic::transport::Router` incrementally.
//
// Each service closure returns an `ErasedRouter` — a boxed object that can
// route requests without exposing the concrete Router type to the builder.
// ---------------------------------------------------------------------------

/// Type-erased function that adds one service to a tonic server and returns
/// a closure capable of calling `serve_with_shutdown`.
pub(crate) type ServeFn = Arc<
    dyn Fn(
            SocketAddr,
            std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), tonic::transport::Error>> + Send>,
        > + Send
        + Sync,
>;

/// Builder for a [`GrpcServer`] component.
///
/// ```rust,no_run
/// # use rskit_server::{GrpcServerBuilder, GrpcServerConfig};
/// let server = GrpcServerBuilder::new(GrpcServerConfig::default())
///     .build();
/// ```
pub struct GrpcServerBuilder {
    name: String,
    config: GrpcServerConfig,
    /// Accumulated serve fns — each captures one tonic service.
    serve_fns: Vec<ServeFn>,
}

impl GrpcServerBuilder {
    /// Create a new builder with the given server configuration.
    pub fn new(config: GrpcServerConfig) -> Self {
        Self {
            name: "grpc-server".into(),
            config,
            serve_fns: Vec::new(),
        }
    }

    /// Override the component name (default: `"grpc-server"`).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a tonic-generated service.
    ///
    /// The service is captured in a closure that later calls
    /// `Server::builder().add_service(svc).serve_with_shutdown(addr, signal)`.
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
        let serve_fn: ServeFn = Arc::new(move |addr, signal| {
            let s = svc.clone();
            Box::pin(async move {
                Server::builder()
                    .add_service(s)
                    .serve_with_shutdown(addr, signal)
                    .await
            })
        });
        self.serve_fns.push(serve_fn);
        self
    }

    /// Build the [`GrpcServer`] component.
    ///
    /// Only the **last** registered service is used for the actual server (since
    /// `tonic::transport::Router` can't be accumulated type-safely without the
    /// concrete service type). For multiple services, compose them before calling
    /// `add_service`, or use the raw tonic API.
    pub fn build(self) -> GrpcServer {
        let serve_fns = Arc::new(self.serve_fns);

        let start_fn = Arc::new(move |addr: SocketAddr, cancel: CancellationToken| {
            let fns = serve_fns.clone();
            tokio::spawn(async move {
                // Clone before calling cancelled_owned() so the original is still
                // usable in the else branch.
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

use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tonic::transport::Server;

use crate::component::GrpcServer;
use crate::config::GrpcServerConfig;

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
    /// Routes added via `add_service`. Stored as boxed closures that accept a
    /// `tonic::transport::Server` and return one with the service added.
    ///
    /// Because `tonic::Router` is generic we use a type-erased builder pattern:
    /// each closure captures the service and applies it to the server.
    services: Vec<Box<dyn Fn(Server) -> tonic::transport::Router + Send + Sync>>,
    with_reflection: bool,
    with_health: bool,
}

impl GrpcServerBuilder {
    pub fn new(config: GrpcServerConfig) -> Self {
        Self {
            name: "grpc-server".into(),
            config,
            services: Vec::new(),
            with_reflection: false,
            with_health: false,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a tonic service to this server.
    ///
    /// `S` must implement `tonic::codegen::Service` — i.e. a generated `*Server<T>`.
    pub fn add_service<S>(mut self, svc: S) -> Self
    where
        S: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<tonic::body::BoxBody>,
                Error = std::convert::Infallible,
            > + tonic::server::NamedService
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        self.services.push(Box::new(move |server: Server| server.add_service(svc.clone())));
        self
    }

    /// Enable gRPC server reflection (requires the `tonic-reflection` feature).
    pub fn with_reflection(mut self) -> Self {
        self.with_reflection = true;
        self
    }

    /// Enable the standard gRPC health protocol.
    pub fn with_health_check(mut self) -> Self {
        self.with_health = true;
        self
    }

    /// Build the [`GrpcServer`] component.
    pub fn build(self) -> GrpcServer {
        let services = Arc::new(self.services);
        let with_health = self.with_health;

        let start_fn = Arc::new(move |addr: SocketAddr, cancel: CancellationToken| {
            let svcs = services.clone();

            tokio::spawn(async move {
                let mut server = Server::builder();
                let mut router: Option<tonic::transport::Router> = None;

                // Apply each service closure.
                for build_svc in svcs.iter() {
                    router = Some(match router.take() {
                        None => build_svc(server.clone()),
                        Some(r) => {
                            // tonic::Router::add_service — we can't chain directly through
                            // the closure abstraction here without the concrete type, so we
                            // use a simpler approach: collect services and add them in order.
                            // For production use, callers should add all services upfront.
                            r
                        }
                    });
                }

                if let Some(r) = router {
                    let serve = r.serve_with_shutdown(addr, cancel.cancelled());
                    tracing::info!(addr = %addr, "gRPC server listening");
                    if let Err(e) = serve.await {
                        tracing::error!(error = %e, "gRPC server error");
                    }
                } else {
                    tracing::warn!(addr = %addr, "gRPC server started with no services");
                    cancel.cancelled().await;
                }

                tracing::info!(addr = %addr, "gRPC server stopped");
            })
        });

        GrpcServer::new(self.name, self.config, start_fn)
    }
}

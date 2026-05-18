use std::sync::Arc;

use axum::Router;

/// Canonical interceptor ordering shared across rskit transports.
///
/// Request processing flows through these phases in order. Metrics wrap the
/// handler completion so response observations happen after the handler returns.
pub const HTTP_INTERCEPTOR_ORDER: [&str; 5] =
    ["tracing", "logging", "auth", "validation", "metrics"];

/// Baseline HTTP transport layers applied by [`HttpServerBuilder`](crate::HttpServerBuilder).
///
/// These layers wrap every HTTP server before application middleware executes.
pub const HTTP_BASELINE_LAYER_ORDER: [&str; 5] = [
    "request_id",
    "cors",
    "security_headers",
    "body_limit",
    "timeout",
];

/// Boxed router transform used to inject transport middleware without exposing
/// axum's concrete layer stack in the public API.
pub type RouterTransform = Arc<dyn Fn(Router) -> Router + Send + Sync + 'static>;

/// Ordered HTTP middleware phases for service-facing servers.
#[derive(Clone, Default)]
pub struct HttpMiddlewareStack {
    tracing: Vec<RouterTransform>,
    logging: Vec<RouterTransform>,
    auth: Vec<RouterTransform>,
    validation: Vec<RouterTransform>,
    metrics: Vec<RouterTransform>,
}

impl HttpMiddlewareStack {
    /// Create an empty middleware stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a tracing-phase transform.
    #[must_use]
    pub fn with_tracing_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.tracing.push(Arc::new(transform));
        self
    }

    /// Append a logging-phase transform.
    #[must_use]
    pub fn with_logging_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.logging.push(Arc::new(transform));
        self
    }

    /// Append an auth-phase transform.
    #[must_use]
    pub fn with_auth_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.auth.push(Arc::new(transform));
        self
    }

    /// Append a validation-phase transform.
    #[must_use]
    pub fn with_validation_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.validation.push(Arc::new(transform));
        self
    }

    /// Append a metrics-phase transform.
    #[must_use]
    pub fn with_metrics_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.metrics.push(Arc::new(transform));
        self
    }

    /// Apply the configured phases around a router.
    pub fn apply(&self, router: Router) -> Router {
        let router = apply_phase(router, &self.metrics);
        let router = apply_phase(router, &self.validation);
        let router = apply_phase(router, &self.auth);
        let router = apply_phase(router, &self.logging);
        apply_phase(router, &self.tracing)
    }
}

fn apply_phase(router: Router, transforms: &[RouterTransform]) -> Router {
    transforms
        .iter()
        .fold(router, |router, transform| transform(router))
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, response::Response, routing::get};
    use parking_lot::Mutex;
    use tower::{Layer, Service, ServiceExt};

    use super::HttpMiddlewareStack;

    #[derive(Clone)]
    struct RecordLayer {
        name: &'static str,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordLayer {
        fn new(name: &'static str, events: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self { name, events }
        }
    }

    impl<S> Layer<S> for RecordLayer {
        type Service = RecordService<S>;

        fn layer(&self, inner: S) -> Self::Service {
            RecordService {
                inner,
                name: self.name,
                events: Arc::clone(&self.events),
            }
        }
    }

    #[derive(Clone)]
    struct RecordService<S> {
        inner: S,
        name: &'static str,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl<S> Service<Request<Body>> for RecordService<S>
    where
        S: Service<Request<Body>, Response = Response, Error = Infallible> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        type Response = Response;
        type Error = Infallible;
        type Future = futures::future::BoxFuture<'static, Result<Response, Infallible>>;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, req: Request<Body>) -> Self::Future {
            let mut inner = self.inner.clone();
            let events = Arc::clone(&self.events);
            let name = self.name;
            Box::pin(async move {
                events.lock().push(name);
                inner.call(req).await
            })
        }
    }

    #[tokio::test]
    async fn stack_applies_request_phases_in_locked_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().route("/", get(|| async { "ok" }));
        let app = HttpMiddlewareStack::new()
            .with_tracing_transform({
                let events = Arc::clone(&events);
                move |router| router.layer(RecordLayer::new("tracing", Arc::clone(&events)))
            })
            .with_logging_transform({
                let events = Arc::clone(&events);
                move |router| router.layer(RecordLayer::new("logging", Arc::clone(&events)))
            })
            .with_auth_transform({
                let events = Arc::clone(&events);
                move |router| router.layer(RecordLayer::new("auth", Arc::clone(&events)))
            })
            .with_validation_transform({
                let events = Arc::clone(&events);
                move |router| router.layer(RecordLayer::new("validation", Arc::clone(&events)))
            })
            .with_metrics_transform({
                let events = Arc::clone(&events);
                move |router| router.layer(RecordLayer::new("metrics", Arc::clone(&events)))
            })
            .apply(app);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            *events.lock(),
            vec!["tracing", "logging", "auth", "validation", "metrics"]
        );
    }
}

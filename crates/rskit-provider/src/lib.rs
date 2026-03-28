//! Provider traits (request-response, stream, sink, duplex) with tower middleware.

#![warn(missing_docs)]

/// Closure-based provider adapters.
pub mod adapt;
/// Tower middleware layers: logging, tracing, resilience.
pub mod middleware;
/// Core provider traits.
pub mod traits;
/// [`TowerProvider`] — bridge from `tower::Service` to [`traits::RequestResponse`].
pub mod tower_bridge;

pub use adapt::{request_response_fn, sink_fn};
pub use traits::{Duplex, DuplexChannel, Provider, RequestResponse, Sink, StreamProvider};
pub use tower_bridge::TowerProvider;

#[cfg(test)]
mod tests {
    use tower::ServiceBuilder;

    use rskit_errors::{AppError, ErrorCode};

    use crate::middleware::logging::LoggingLayer;
    use crate::middleware::resilience::{ResilienceConfig, ResilienceLayer};
    use crate::traits::{Provider, RequestResponse, Sink};
    use crate::{request_response_fn, sink_fn, TowerProvider};
    use rskit_resilience::RateLimiter;

    // ── 1. request_response_fn_executes ──────────────────────────────────────

    #[tokio::test]
    async fn request_response_fn_executes() {
        let provider = request_response_fn("test", |x: i32| async move { Ok(x * 2) });
        let result = provider.execute(21).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(provider.name(), "test");
    }

    // ── 2. sink_fn_sends_value ────────────────────────────────────────────────

    #[tokio::test]
    async fn sink_fn_sends_value() {
        let sink = sink_fn("sink", |_x: i32| async move { Ok(()) });
        let result = sink.send(1).await;
        assert_eq!(result.unwrap(), ());
    }

    // ── 3. tower_provider_executes ────────────────────────────────────────────

    #[tokio::test]
    async fn tower_provider_executes() {
        let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req * 2) });
        let provider = TowerProvider::new("tp", svc);
        let result = provider.execute(5).await;
        assert_eq!(result.unwrap(), 10);
    }

    // ── 4. logging_layer_passes_through ──────────────────────────────────────

    #[tokio::test]
    async fn logging_layer_passes_through() {
        use tower::Service;

        let inner = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req + 1) });
        let mut svc = ServiceBuilder::new()
            .layer(LoggingLayer::new("test"))
            .service(inner);

        // Poll ready first (as required by Tower's contract)
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        let result = svc.call(10).await;
        assert_eq!(result.unwrap(), 11);
    }

    // ── 5. resilience_layer_rate_limits ──────────────────────────────────────

    #[tokio::test]
    async fn resilience_layer_rate_limits() {
        use tower::Service;

        let inner = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });
        let config = ResilienceConfig::new()
            .with_rate_limiter(RateLimiter::new("test-rl", 1, 1));

        let mut svc = ServiceBuilder::new()
            .layer(ResilienceLayer::new(config))
            .service(inner);

        // First call — should succeed (consumes the single burst token)
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        let first = svc.call(1).await;
        assert!(first.is_ok(), "first call should succeed");

        // Second call — bucket is exhausted, should be rate-limited
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        let second = svc.call(2).await;
        let err = second.expect_err("second call should be rate-limited");
        assert_eq!(err.code, ErrorCode::RateLimited);
    }
}

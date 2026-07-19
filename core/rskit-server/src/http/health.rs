use std::sync::Arc;

use axum::Router;
use rskit_bootstrap::Registry;

/// Add a `/health` endpoint returning JSON from a [`Registry`].
pub fn health_router(registry: Arc<Registry>) -> Router {
    use axum::{Json, routing::get};

    Router::new().route(
        "/health",
        get({
            let registry = Arc::clone(&registry);
            move || {
                let registry = Arc::clone(&registry);
                async move {
                    let healths = registry.health_all();
                    let all_ok = healths.iter().all(|health| health.is_healthy());
                    let status = if all_ok {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    };
                    (status, Json(healths))
                }
            }
        }),
    )
}

/// Returns a router with a `/healthz` liveness probe endpoint.
pub fn healthz_router() -> Router {
    use axum::{Json, routing::get};
    use serde::Serialize;

    #[derive(Serialize)]
    struct LivenessResponse {
        status: &'static str,
        version: &'static str,
    }

    async fn healthz_handler() -> Json<LivenessResponse> {
        Json(LivenessResponse {
            status: "ok",
            version: rskit_version::package_version(),
        })
    }

    Router::new().route("/healthz", get(healthz_handler))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use rskit_bootstrap::Registry;
    use tower::ServiceExt;

    use super::{health_router, healthz_router};

    #[tokio::test]
    async fn health_routers_report_registry_and_liveness() {
        let registry = Arc::new(Registry::new());
        let health = health_router(registry);
        let response = health
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("health request"),
            )
            .await
            .expect("health response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health body");
        assert_eq!(&body[..], b"[]");

        let liveness = healthz_router();
        let response = liveness
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("healthz request"),
            )
            .await
            .expect("healthz response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("healthz body");
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("\"status\":\"ok\""));
        assert!(body.contains("\"version\""));
    }
}

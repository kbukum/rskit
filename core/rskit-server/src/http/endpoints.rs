//! Standard service endpoints: health, readiness, liveness, build info, and metrics.
//!
//! The probe wire shapes are aligned across kits: `/health` (and its `/healthz` alias) return
//! `{status, service, timestamp, components[]}` with `200` for healthy/degraded and `503` for
//! unhealthy; `/livez` and `/readyz` are lightweight probes with the same envelope. Component
//! health reuses the canonical [`rskit_bootstrap::Health`]/[`rskit_bootstrap::HealthStatus`]
//! vocabulary (`healthy`/`degraded`/`unhealthy`).
//!
//! `/info` and `/metrics` are language-idiomatic rather than wire-identical: `/info` reports Rust
//! build metadata (`rust_version`, integer `uptime_seconds`) and `/metrics` returns a minimal
//! runtime snapshot. Detailed telemetry is exported via OTLP by `rskit-observability`; these two
//! endpoints are pinned by regression golden fixtures, not the cross-kit parity set.

use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use rskit_bootstrap::{Health, HealthStatus, Registry};
use rskit_version::VersionInfo;
use serde::Serialize;

/// Shared state for the standard service endpoints.
#[derive(Clone)]
struct EndpointsState {
    registry: Arc<Registry>,
    service: Arc<str>,
    start: Instant,
}

/// Build a router exposing the standard service endpoints for `service`.
///
/// Mounts `/health`, `/healthz`, `/livez`, `/readyz`, `/info`, and `/metrics`. Component
/// health is sourced from `registry`; uptime is measured from the moment this router is built.
pub fn observability_router(registry: Arc<Registry>, service: impl Into<String>) -> Router {
    let state = EndpointsState {
        registry,
        service: Arc::from(service.into().as_str()),
        start: Instant::now(),
    };

    Router::new()
        .route("/health", get(health_handler))
        .route("/healthz", get(health_handler))
        .route("/livez", get(liveness_handler))
        .route("/readyz", get(readiness_handler))
        .route("/info", get(info_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Full health document: overall status plus per-component reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct HealthResponse {
    status: HealthStatus,
    service: String,
    timestamp: String,
    components: Vec<Health>,
}

/// Liveness probe payload; `status` is always `"alive"`.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct LivenessResponse {
    status: &'static str,
    service: String,
    timestamp: String,
}

/// Readiness probe payload; `status` is `"ready"` or `"not_ready"`.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct ReadinessResponse {
    status: &'static str,
    service: String,
    timestamp: String,
}

/// Build/version metadata plus process uptime.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct InfoResponse {
    #[serde(flatten)]
    version: VersionInfo,
    service: String,
    uptime_seconds: u64,
    timestamp: String,
}

/// Minimal runtime snapshot. Detailed telemetry is exported via OTLP by `rskit-observability`;
/// this endpoint offers a deterministic, dependency-free summary for scrape/liveness tooling.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct MetricsResponse {
    service: String,
    uptime_seconds: u64,
    timestamp: String,
}

/// Worst status across a set of component reports; an empty set is [`HealthStatus::Healthy`].
fn overall_status(components: &[Health]) -> HealthStatus {
    if components
        .iter()
        .any(|c| c.status == HealthStatus::Unhealthy)
    {
        HealthStatus::Unhealthy
    } else if components
        .iter()
        .any(|c| c.status == HealthStatus::Degraded)
    {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    }
}

/// HTTP status for a health document: `503` only when the overall status is unhealthy.
fn health_http_status(status: &HealthStatus) -> StatusCode {
    match status {
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::OK,
    }
}

/// Current UTC timestamp in RFC 3339 form.
///
/// Returns `None` when the system clock is unavailable (set before the Unix epoch or beyond the
/// representable range); handlers surface that as `500` rather than emitting an invalid timestamp.
fn now_timestamp() -> Option<String> {
    rskit_util::time::now_rfc3339()
}

impl EndpointsState {
    fn uptime_seconds(&self) -> u64 {
        self.start.elapsed().as_secs()
    }

    fn health_response(&self, timestamp: String) -> HealthResponse {
        let components = self.registry.health_all();
        HealthResponse {
            status: overall_status(&components),
            service: self.service.to_string(),
            timestamp,
            components,
        }
    }
}

async fn health_handler(
    State(state): State<EndpointsState>,
) -> Result<(StatusCode, Json<HealthResponse>), StatusCode> {
    let timestamp = now_timestamp().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = state.health_response(timestamp);
    Ok((health_http_status(&response.status), Json(response)))
}

async fn liveness_handler(
    State(state): State<EndpointsState>,
) -> Result<Json<LivenessResponse>, StatusCode> {
    let timestamp = now_timestamp().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(LivenessResponse {
        status: "alive",
        service: state.service.to_string(),
        timestamp,
    }))
}

async fn readiness_handler(
    State(state): State<EndpointsState>,
) -> Result<(StatusCode, Json<ReadinessResponse>), StatusCode> {
    let timestamp = now_timestamp().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ready = overall_status(&state.registry.health_all()) != HealthStatus::Unhealthy;
    let (status, label) = if ready {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not_ready")
    };
    Ok((
        status,
        Json(ReadinessResponse {
            status: label,
            service: state.service.to_string(),
            timestamp,
        }),
    ))
}

async fn info_handler(
    State(state): State<EndpointsState>,
) -> Result<Json<InfoResponse>, StatusCode> {
    let timestamp = now_timestamp().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(InfoResponse {
        version: rskit_version::get_version_info(),
        service: state.service.to_string(),
        uptime_seconds: state.uptime_seconds(),
        timestamp,
    }))
}

async fn metrics_handler(
    State(state): State<EndpointsState>,
) -> Result<Json<MetricsResponse>, StatusCode> {
    let timestamp = now_timestamp().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(MetricsResponse {
        service: state.service.to_string(),
        uptime_seconds: state.uptime_seconds(),
        timestamp,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use rskit_bootstrap::{Health, HealthStatus, Registry};
    use rskit_version::VersionInfo;
    use tower::ServiceExt;

    use super::{
        HealthResponse, InfoResponse, LivenessResponse, MetricsResponse, ReadinessResponse,
        observability_router, overall_status,
    };

    fn fixed_version() -> VersionInfo {
        VersionInfo {
            version: "1.2.3".to_string(),
            git_commit: "abc123".to_string(),
            git_branch: "main".to_string(),
            build_time: "2024-01-01T00:00:00Z".to_string(),
            build_date: Some("2024-01-01".to_string()),
            rust_version: "rustc 1.97.0".to_string(),
            is_release: true,
            is_dirty: false,
        }
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[test]
    fn overall_status_reports_worst_component() {
        assert_eq!(overall_status(&[]), HealthStatus::Healthy);
        assert_eq!(
            overall_status(&[Health::healthy("a"), Health::degraded("b", "slow")]),
            HealthStatus::Degraded
        );
        assert_eq!(
            overall_status(&[
                Health::degraded("a", "slow"),
                Health::unhealthy("b", "down")
            ]),
            HealthStatus::Unhealthy
        );
    }

    #[test]
    fn health_response_matches_cross_kit_golden_fixture() {
        let response = HealthResponse {
            status: HealthStatus::Degraded,
            service: "orders".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            components: vec![
                Health::healthy("cache"),
                Health::degraded("db", "high latency"),
            ],
        };
        let actual = serde_json::to_string_pretty(&response).unwrap();
        let expected = include_str!("../../tests/fixtures/cross-kit/server/health.json");
        assert_eq!(format!("{actual}\n"), expected);
    }

    #[test]
    fn liveness_response_matches_cross_kit_golden_fixture() {
        let response = LivenessResponse {
            status: "alive",
            service: "orders".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let actual = serde_json::to_string_pretty(&response).unwrap();
        let expected = include_str!("../../tests/fixtures/cross-kit/server/liveness.json");
        assert_eq!(format!("{actual}\n"), expected);
    }

    #[test]
    fn readiness_response_matches_cross_kit_golden_fixture() {
        let response = ReadinessResponse {
            status: "ready",
            service: "orders".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let actual = serde_json::to_string_pretty(&response).unwrap();
        let expected = include_str!("../../tests/fixtures/cross-kit/server/readiness.json");
        assert_eq!(format!("{actual}\n"), expected);
    }

    #[test]
    fn info_response_matches_golden_fixture() {
        let response = InfoResponse {
            version: fixed_version(),
            service: "orders".to_string(),
            uptime_seconds: 42,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let actual = serde_json::to_string_pretty(&response).unwrap();
        let expected = include_str!("../../tests/fixtures/golden/server/info.json");
        assert_eq!(format!("{actual}\n"), expected);
    }

    #[test]
    fn metrics_response_matches_golden_fixture() {
        let response = MetricsResponse {
            service: "orders".to_string(),
            uptime_seconds: 42,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let actual = serde_json::to_string_pretty(&response).unwrap();
        let expected = include_str!("../../tests/fixtures/golden/server/metrics.json");
        assert_eq!(format!("{actual}\n"), expected);
    }

    #[tokio::test]
    async fn health_and_healthz_return_ok_for_empty_registry() {
        let router = observability_router(Arc::new(Registry::new()), "orders");
        for uri in ["/health", "/healthz"] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let json = body_json(response).await;
            assert_eq!(json["status"], "healthy");
            assert_eq!(json["service"], "orders");
            assert!(json["components"].is_array());
        }
    }

    #[tokio::test]
    async fn health_returns_unavailable_when_component_unhealthy() {
        use async_trait::async_trait;
        use rskit_bootstrap::Component;

        #[derive(Debug)]
        struct DownComponent;

        #[async_trait]
        impl Component for DownComponent {
            fn name(&self) -> &str {
                "database"
            }
            async fn start(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }
            async fn stop(&self) -> rskit_errors::AppResult<()> {
                Ok(())
            }
            fn health(&self) -> Health {
                Health::unhealthy("database", "connection refused")
            }
        }

        let mut registry = Registry::new();
        registry.register(Arc::new(DownComponent));
        let router = observability_router(Arc::new(registry), "orders");

        let health = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::SERVICE_UNAVAILABLE);
        let json = body_json(health).await;
        assert_eq!(json["status"], "unhealthy");
        assert_eq!(json["components"][0]["name"], "database");

        let ready = router
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body_json(ready).await["status"], "not_ready");
    }

    #[tokio::test]
    async fn liveness_readiness_info_and_metrics_expose_expected_fields() {
        let router = observability_router(Arc::new(Registry::new()), "orders");

        let livez = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(livez.status(), StatusCode::OK);
        assert_eq!(body_json(livez).await["status"], "alive");

        let readyz = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(readyz.status(), StatusCode::OK);
        assert_eq!(body_json(readyz).await["status"], "ready");

        let info = router
            .clone()
            .oneshot(Request::builder().uri("/info").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(info.status(), StatusCode::OK);
        let info_json = body_json(info).await;
        assert_eq!(info_json["service"], "orders");
        assert!(info_json["rust_version"].is_string());
        assert!(info_json["uptime_seconds"].is_u64());

        let metrics = router
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(metrics.status(), StatusCode::OK);
        assert!(body_json(metrics).await["uptime_seconds"].is_u64());
    }
}

//! Tenant ID extraction for multi-tenant applications.
//!
//! This module provides:
//! - [`TenantId`] extractor for Axum handlers that reads tenant ID from
//!   the "x-tenant-id" header.
//! - [`set_tenant_in_extensions`] and [`tenant_from_extensions`] helpers for
//!   storing / retrieving a tenant ID in HTTP request extensions.
//! - [`tenant_middleware`] — an Axum middleware that automatically extracts the
//!   tenant header and inserts the value into request extensions.

use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{Extensions, Request, Response, StatusCode, request::Parts},
    middleware::Next,
};

/// Newtype wrapping a tenant ID string.
///
/// Use in Axum handler signatures to extract tenant ID from request headers:
///
/// ```ignore
/// async fn handler(TenantId(id): TenantId) -> impl IntoResponse {
///     format!("Tenant: {}", id)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

impl<S: Send + Sync> FromRequestParts<S> for TenantId {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .ok_or((StatusCode::UNAUTHORIZED, "missing x-tenant-id header"))?;
        Ok(TenantId(id))
    }
}

/// Configuration for tenant extraction via Tower middleware.
#[derive(Debug, Clone)]
pub struct TenantConfig {
    /// Header name to read tenant ID from (default: "x-tenant-id").
    pub header_name: String,
    /// If true, requests without tenant ID are rejected.
    pub required: bool,
    /// Optional default tenant ID when header is missing and not required.
    pub fallback: Option<String>,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            header_name: "x-tenant-id".to_string(),
            required: false,
            fallback: None,
        }
    }
}

// ── Extension helpers ─────────────────────────────────────────────────────────

/// Store a [`TenantId`] in request extensions.
///
/// This is useful when composing custom middleware that needs to set the tenant
/// value without using the full [`tenant_middleware`].
pub fn set_tenant_in_extensions(extensions: &mut Extensions, tenant_id: impl Into<String>) {
    extensions.insert(TenantId(tenant_id.into()));
}

/// Retrieve the [`TenantId`] from request extensions.
///
/// Returns `None` when no tenant has been stored.
pub fn tenant_from_extensions(extensions: &Extensions) -> Option<&TenantId> {
    extensions.get::<TenantId>()
}

// ── Axum middleware ───────────────────────────────────────────────────────────

/// Axum middleware that extracts a tenant ID from request headers and stores it
/// in request extensions.
///
/// Behaviour depends on [`TenantConfig`]:
/// - Header present → [`TenantId`] inserted into extensions.
/// - Header missing + `required` → responds with `400 Bad Request`.
/// - Header missing + `fallback` set → fallback value inserted.
/// - Header missing + not required + no fallback → request continues without a
///   tenant.
///
/// # Example
///
/// ```ignore
/// use axum::{Router, middleware};
/// use rskit_http::tenant::{TenantConfig, tenant_middleware};
///
/// let cfg = TenantConfig::default();
/// let app = Router::new()
///     .layer(middleware::from_fn(move |req, next| {
///         tenant_middleware(cfg.clone(), req, next)
///     }));
/// ```
pub async fn tenant_middleware(
    config: TenantConfig,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, (StatusCode, &'static str)> {
    let tenant_id = req
        .headers()
        .get(&config.header_name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    match tenant_id {
        Some(id) => {
            req.extensions_mut().insert(TenantId(id));
        }
        None if config.required => {
            return Err((StatusCode::BAD_REQUEST, "tenant ID required"));
        }
        None => {
            if let Some(fallback) = &config.fallback {
                req.extensions_mut().insert(TenantId(fallback.clone()));
            }
        }
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    // ── Extractor tests ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_tenant_id_from_header() {
        let req = Request::builder()
            .header("x-tenant-id", "tenant-123")
            .body(())
            .unwrap();

        let (mut parts, _) = req.into_parts();
        let result = TenantId::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        let TenantId(id) = result.unwrap();
        assert_eq!(id, "tenant-123");
    }

    #[tokio::test]
    async fn test_tenant_id_missing() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let result = TenantId::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(msg, "missing x-tenant-id header");
    }

    #[test]
    fn test_tenant_config_default() {
        let cfg = TenantConfig::default();
        assert_eq!(cfg.header_name, "x-tenant-id");
        assert!(!cfg.required);
        assert!(cfg.fallback.is_none());
    }

    #[test]
    fn test_tenant_config_with_fallback() {
        let cfg = TenantConfig {
            header_name: "x-tenant-id".to_string(),
            required: false,
            fallback: Some("default".to_string()),
        };
        assert_eq!(cfg.fallback, Some("default".to_string()));
    }

    // ── Extension helper tests ────────────────────────────────────────────────

    #[test]
    fn test_set_and_get_tenant_in_extensions() {
        let mut ext = Extensions::new();
        set_tenant_in_extensions(&mut ext, "org-42");

        let tenant = tenant_from_extensions(&ext);
        assert!(tenant.is_some());
        assert_eq!(tenant.unwrap().0, "org-42");
    }

    #[test]
    fn test_tenant_from_extensions_empty() {
        let ext = Extensions::new();
        assert!(tenant_from_extensions(&ext).is_none());
    }

    #[test]
    fn test_set_tenant_overwrites_previous() {
        let mut ext = Extensions::new();
        set_tenant_in_extensions(&mut ext, "first");
        set_tenant_in_extensions(&mut ext, "second");
        assert_eq!(tenant_from_extensions(&ext).unwrap().0, "second");
    }

    // ── Middleware tests ──────────────────────────────────────────────────────

    /// Build a minimal Axum app that runs the tenant middleware and echoes the
    /// tenant ID (or "none") so we can assert against the response body.
    fn test_app(config: TenantConfig) -> axum::Router {
        use axum::{Router, middleware, routing::get};

        async fn echo_tenant(req: Request<Body>) -> String {
            match req.extensions().get::<TenantId>() {
                Some(TenantId(id)) => id.clone(),
                None => "none".to_string(),
            }
        }

        Router::new()
            .route("/", get(echo_tenant))
            .layer(middleware::from_fn(move |req, next| {
                tenant_middleware(config.clone(), req, next)
            }))
    }

    #[tokio::test]
    async fn test_middleware_header_present() {
        use tower::ServiceExt;

        let app = test_app(TenantConfig::default());
        let req = Request::builder()
            .uri("/")
            .header("x-tenant-id", "acme")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "acme");
    }

    #[tokio::test]
    async fn test_middleware_missing_required() {
        use tower::ServiceExt;

        let cfg = TenantConfig {
            required: true,
            ..Default::default()
        };
        let app = test_app(cfg);
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_middleware_missing_with_fallback() {
        use tower::ServiceExt;

        let cfg = TenantConfig {
            fallback: Some("default-tenant".to_string()),
            ..Default::default()
        };
        let app = test_app(cfg);
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "default-tenant");
    }

    #[tokio::test]
    async fn test_middleware_missing_not_required_no_fallback() {
        use tower::ServiceExt;

        let app = test_app(TenantConfig::default());
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "none");
    }
}

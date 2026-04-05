//! Tenant ID extraction for multi-tenant applications.
//!
//! This module provides [`TenantId`] extractor for Axum handlers that reads
//! tenant ID from the "x-tenant-id" header. It integrates with `rskit-errors`
//! for consistent error handling.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

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
}

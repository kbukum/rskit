//! Tenant ID helpers for multi-tenant HTTP applications.

use http::{Extensions, HeaderMap};

/// Newtype wrapping a tenant ID string.
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

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

impl TenantConfig {
    /// Read a tenant id from headers according to this config.
    #[must_use]
    pub fn tenant_from_headers(&self, headers: &HeaderMap) -> Option<TenantId> {
        headers
            .get(&self.header_name)
            .and_then(|value| value.to_str().ok())
            .map(|value| TenantId(value.to_owned()))
            .or_else(|| self.fallback.clone().map(TenantId))
    }

    /// Returns whether a request without tenant information should be rejected.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }
}

/// Store a [`TenantId`] in request extensions.
///
/// This is useful when composing custom middleware that needs to set the tenant value.
pub fn set_tenant_in_extensions(extensions: &mut Extensions, tenant_id: impl Into<String>) {
    extensions.insert(TenantId(tenant_id.into()));
}

/// Retrieve the [`TenantId`] from request extensions.
///
/// Returns `None` when no tenant has been stored.
pub fn tenant_from_extensions(extensions: &Extensions) -> Option<&TenantId> {
    extensions.get::<TenantId>()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_tenant_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-tenant-id", "tenant-123".parse().unwrap());
        let tenant = TenantConfig::default().tenant_from_headers(&headers);
        assert_eq!(tenant.unwrap().0, "tenant-123");
    }

    #[test]
    fn test_tenant_from_headers_uses_fallback() {
        let cfg = TenantConfig {
            fallback: Some("default".to_string()),
            ..Default::default()
        };
        assert_eq!(
            cfg.tenant_from_headers(&HeaderMap::new()).unwrap().0,
            "default"
        );
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
}

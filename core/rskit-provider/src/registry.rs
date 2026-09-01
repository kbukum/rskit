//! Provider registry with operation and tier-based resolution.

use std::collections::HashMap;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::Provider;

/// Binding of an operation to a provider with priority and tier access control.
#[derive(Debug, Clone)]
pub struct Binding<P> {
    /// Identifier of the operation this binding serves.
    pub operation_id: String,
    /// The provider instance.
    pub provider: P,
    /// Allowed tiers. An empty list means *all* tiers.
    pub tiers: Vec<String>,
    /// Lower values are preferred during resolution.
    pub priority: i32,
}

/// Registry that resolves providers for operations based on tier and priority.
///
/// Resolution:
/// 1. Look up bindings by `operation_id`.
/// 2. Filter to bindings whose `tiers` list is empty (wildcard) or contains the requested tier.
/// 3. Skip providers that report [`Provider::is_available`] as `false`.
/// 4. Return the binding with the lowest `priority` value.
pub struct Registry<P> {
    bindings: HashMap<String, Vec<Binding<P>>>,
}

impl<P: Clone> Default for Registry<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Clone> Registry<P> {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Register a provider binding for an operation.
    pub fn bind(&mut self, binding: Binding<P>) {
        self.bindings
            .entry(binding.operation_id.clone())
            .or_default()
            .push(binding);
    }

    /// List all bindings registered for an operation.
    ///
    /// Returns an empty slice when the operation is unknown.
    pub fn list_bindings(&self, operation_id: &str) -> &[Binding<P>] {
        self.bindings
            .get(operation_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

impl<P: Provider + Clone> Registry<P> {
    /// Resolve the best provider for `operation_id` given the caller's `tier`.
    ///
    /// Returns the provider from the highest-priority (lowest `priority` value) binding that
    /// matches the tier and is currently [available](Provider::is_available).
    ///
    /// Errors distinguish absence from unavailability, matching the cross-kit provider contract:
    /// a [`NotFound`](ErrorCode::NotFound) when no binding serves the operation for the tier, and a
    /// retryable [`ServiceUnavailable`](ErrorCode::ServiceUnavailable) when tier-eligible bindings
    /// exist but every one currently reports [unavailable](Provider::is_available).
    pub fn resolve(&self, operation_id: &str, tier: &str) -> AppResult<&P> {
        let bindings = self.bindings.get(operation_id).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("no bindings registered for operation '{operation_id}'"),
            )
        })?;

        let mut eligible = bindings
            .iter()
            .filter(|b| b.tiers.is_empty() || b.tiers.iter().any(|t| t == tier))
            .peekable();
        if eligible.peek().is_none() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("no provider for operation '{operation_id}' accessible to tier '{tier}'"),
            ));
        }

        eligible
            .filter(|b| b.provider.is_available())
            .min_by_key(|b| b.priority)
            .map(|b| &b.provider)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!(
                        "no available provider for operation '{operation_id}' accessible to tier '{tier}'"
                    ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestProvider {
        name: &'static str,
        available: bool,
    }

    impl TestProvider {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                available: true,
            }
        }

        fn unavailable(name: &'static str) -> Self {
            Self {
                name,
                available: false,
            }
        }
    }

    impl Provider for TestProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn is_available(&self) -> bool {
            self.available
        }
    }

    fn binding(
        operation_id: &str,
        provider: TestProvider,
        tiers: &[&str],
        priority: i32,
    ) -> Binding<TestProvider> {
        Binding {
            operation_id: operation_id.into(),
            provider,
            tiers: tiers.iter().map(|t| (*t).to_string()).collect(),
            priority,
        }
    }

    #[test]
    fn resolve_returns_highest_priority_match() {
        let mut reg = Registry::new();
        reg.bind(binding(
            "transcode",
            TestProvider::new("slow-provider"),
            &[],
            10,
        ));
        reg.bind(binding(
            "transcode",
            TestProvider::new("fast-provider"),
            &[],
            1,
        ));

        let p = reg.resolve("transcode", "free").unwrap();
        assert_eq!(p.name(), "fast-provider");
    }

    #[test]
    fn resolve_filters_by_tier() {
        let mut reg = Registry::new();
        reg.bind(binding(
            "upscale",
            TestProvider::new("premium-backend"),
            &["pro"],
            1,
        ));
        reg.bind(binding(
            "upscale",
            TestProvider::new("basic-backend"),
            &[],
            5,
        ));

        // "free" tier cannot access premium, falls through to basic
        let p = reg.resolve("upscale", "free").unwrap();
        assert_eq!(p.name(), "basic-backend");

        // "pro" tier gets the preferred premium provider
        let p = reg.resolve("upscale", "pro").unwrap();
        assert_eq!(p.name(), "premium-backend");
    }

    #[test]
    fn resolve_skips_unavailable_providers() {
        let mut reg = Registry::new();
        reg.bind(binding(
            "encode",
            TestProvider::unavailable("preferred-but-down"),
            &[],
            1,
        ));
        reg.bind(binding(
            "encode",
            TestProvider::new("healthy-backup"),
            &[],
            5,
        ));

        let p = reg.resolve("encode", "free").unwrap();
        assert_eq!(p.name(), "healthy-backup");
    }

    #[test]
    fn resolve_all_unavailable_returns_service_unavailable() {
        let mut reg = Registry::new();
        reg.bind(binding("encode", TestProvider::unavailable("down"), &[], 1));

        let err = reg.resolve("encode", "free").unwrap_err();
        assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
        assert!(err.code().is_retryable());
    }

    #[test]
    fn resolve_unknown_operation_returns_not_found() {
        let reg = Registry::<TestProvider>::new();
        let err = reg.resolve("nonexistent", "free").unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
    }

    #[test]
    fn resolve_no_tier_match_returns_not_found() {
        let mut reg = Registry::new();
        reg.bind(binding(
            "export",
            TestProvider::new("enterprise-only"),
            &["enterprise"],
            1,
        ));

        let err = reg.resolve("export", "free").unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
    }

    #[test]
    fn list_bindings_returns_all_registered() {
        let mut reg = Registry::new();
        reg.bind(binding("encode", TestProvider::new("a"), &[], 1));
        reg.bind(binding("encode", TestProvider::new("b"), &["pro"], 2));

        assert_eq!(reg.list_bindings("encode").len(), 2);
        assert!(reg.list_bindings("unknown").is_empty());
    }

    #[test]
    fn default_creates_empty_registry() {
        let reg = Registry::<TestProvider>::default();
        assert!(reg.list_bindings("any").is_empty());
    }
}

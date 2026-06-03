//! Provider registry with operation and tier-based resolution.

use std::collections::HashMap;

use rskit_errors::{AppError, AppResult, ErrorCode};

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
/// 2. Filter to bindings whose `tiers` list is empty (wildcard) or contains the
///    requested tier.
/// 3. Return the binding with the lowest `priority` value.
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

    /// Resolve the best provider for `operation_id` given the caller's `tier`.
    ///
    /// Returns the provider from the highest-priority (lowest `priority` value)
    /// binding that matches the tier.
    pub fn resolve(&self, operation_id: &str, tier: &str) -> AppResult<&P> {
        let bindings = self.bindings.get(operation_id).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("no bindings registered for operation '{operation_id}'"),
            )
        })?;

        bindings
            .iter()
            .filter(|b| b.tiers.is_empty() || b.tiers.iter().any(|t| t == tier))
            .min_by_key(|b| b.priority)
            .map(|b| &b.provider)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!(
                        "no provider for operation '{operation_id}' accessible to tier '{tier}'"
                    ),
                )
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_highest_priority_match() {
        let mut reg = Registry::new();
        reg.bind(Binding {
            operation_id: "transcode".into(),
            provider: "slow-provider",
            tiers: vec![],
            priority: 10,
        });
        reg.bind(Binding {
            operation_id: "transcode".into(),
            provider: "fast-provider",
            tiers: vec![],
            priority: 1,
        });

        let p = reg.resolve("transcode", "free").unwrap();
        assert_eq!(*p, "fast-provider");
    }

    #[test]
    fn resolve_filters_by_tier() {
        let mut reg = Registry::new();
        reg.bind(Binding {
            operation_id: "upscale".into(),
            provider: "premium-backend",
            tiers: vec!["pro".into()],
            priority: 1,
        });
        reg.bind(Binding {
            operation_id: "upscale".into(),
            provider: "basic-backend",
            tiers: vec![],
            priority: 5,
        });

        // "free" tier cannot access premium, falls through to basic
        let p = reg.resolve("upscale", "free").unwrap();
        assert_eq!(*p, "basic-backend");

        // "pro" tier gets the preferred premium provider
        let p = reg.resolve("upscale", "pro").unwrap();
        assert_eq!(*p, "premium-backend");
    }

    #[test]
    fn resolve_unknown_operation_returns_not_found() {
        let reg = Registry::<&str>::new();
        let err = reg.resolve("nonexistent", "free").unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
    }

    #[test]
    fn resolve_no_tier_match_returns_not_found() {
        let mut reg = Registry::new();
        reg.bind(Binding {
            operation_id: "export".into(),
            provider: "enterprise-only",
            tiers: vec!["enterprise".into()],
            priority: 1,
        });

        let err = reg.resolve("export", "free").unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
    }

    #[test]
    fn list_bindings_returns_all_registered() {
        let mut reg = Registry::new();
        reg.bind(Binding {
            operation_id: "encode".into(),
            provider: "a",
            tiers: vec![],
            priority: 1,
        });
        reg.bind(Binding {
            operation_id: "encode".into(),
            provider: "b",
            tiers: vec!["pro".into()],
            priority: 2,
        });

        assert_eq!(reg.list_bindings("encode").len(), 2);
        assert!(reg.list_bindings("unknown").is_empty());
    }

    #[test]
    fn default_creates_empty_registry() {
        let reg = Registry::<String>::default();
        assert!(reg.list_bindings("any").is_empty());
    }
}

//! Provider-agnostic workload configuration.
//!
//! Mirrors gokit's `workload.Config` — the same shape across kits
//! so a workload app is structurally identical regardless of language.

use std::collections::HashMap;

use rskit_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// Provider filled in by [`WorkloadConfig::default`]
/// and [`WorkloadConfig::apply_defaults`] when [`WorkloadConfig::provider`] is empty.
///
/// [`WorkloadConfig::validate`] still rejects an empty provider,
/// so call `apply_defaults` before validating configs that may omit it.
pub const DEFAULT_PROVIDER: &str = "docker";

/// Provider-agnostic workload configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct WorkloadConfig {
    /// Whether the workload component is active. When `false`, the component starts as a healthy no-op
    /// and builds no backend.
    pub enabled: bool,
    /// Backend name looked up in an injected [`crate::WorkloadRegistry`].
    pub provider: String,
    /// Labels applied to every workload the manager deploys.
    pub default_labels: HashMap<String, String>,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: DEFAULT_PROVIDER.to_owned(),
            default_labels: HashMap::new(),
        }
    }
}

impl WorkloadConfig {
    /// Normalize `provider` by trimming surrounding whitespace, filling it with the default when empty,
    /// so stored and logged config is deterministic.
    pub fn apply_defaults(&mut self) {
        let trimmed = self.provider.trim();
        if trimmed.is_empty() {
            DEFAULT_PROVIDER.clone_into(&mut self.provider);
        } else if trimmed.len() != self.provider.len() {
            self.provider = trimmed.to_owned();
        }
    }

    /// Validate the core configuration.
    ///
    /// # Errors
    ///
    /// Returns [`rskit_errors::ErrorCode::MissingField`] when `provider` is empty.
    pub fn validate(&self) -> AppResult<()> {
        if self.provider.trim().is_empty() {
            return Err(AppError::missing_field("workload.provider"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled_with_docker_provider() {
        let cfg = WorkloadConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.provider, "docker");
        assert!(cfg.default_labels.is_empty());
    }

    #[test]
    fn apply_defaults_fills_empty_provider() {
        let mut cfg = WorkloadConfig {
            provider: "   ".to_string(),
            ..Default::default()
        };
        cfg.apply_defaults();
        assert_eq!(cfg.provider, "docker");
    }

    #[test]
    fn apply_defaults_keeps_explicit_provider() {
        let mut cfg = WorkloadConfig {
            provider: "kubernetes".to_string(),
            ..Default::default()
        };
        cfg.apply_defaults();
        assert_eq!(cfg.provider, "kubernetes");
    }

    #[test]
    fn apply_defaults_trims_surrounding_whitespace() {
        let mut cfg = WorkloadConfig {
            provider: "  docker  ".to_string(),
            ..Default::default()
        };
        cfg.apply_defaults();
        assert_eq!(cfg.provider, "docker");
    }

    #[test]
    fn validate_rejects_empty_provider() {
        let cfg = WorkloadConfig {
            provider: String::new(),
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::MissingField);
    }

    #[test]
    fn validate_accepts_named_provider() {
        let cfg = WorkloadConfig {
            provider: "docker".to_string(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }
}

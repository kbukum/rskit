//! Graceful degradation manager for tracking dependency health.
//!
//! [`DegradationManager`] tracks the health of named services and feature
//! flags. It integrates with [`CircuitBreaker`](crate::CircuitBreaker) via
//! [`on_cb_state_change`](DegradationManager::on_cb_state_change) to
//! automatically map circuit breaker states to service health levels.
//!
//! # Example
//!
//! ```rust
//! use rskit_resilience::{DegradationManager, ServiceHealth, CbConfig};
//!
//! let dm = DegradationManager::new();
//!
//! // Manual update
//! dm.update_service("database", ServiceHealth::Healthy, None);
//!
//! // Wire to circuit breaker
//! let cb_config = CbConfig {
//!     on_state_change: Some(dm.on_cb_state_change("database")),
//!     ..Default::default()
//! };
//!
//! assert!(dm.is_healthy());
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::Serialize;

use crate::circuit_breaker::CbState;

/// Health level of a tracked service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceHealth {
    /// Service is operating normally.
    Healthy,
    /// Service is partially available.
    Degraded,
    /// Service is unavailable.
    Unhealthy,
}

impl std::fmt::Display for ServiceHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => f.write_str("healthy"),
            Self::Degraded => f.write_str("degraded"),
            Self::Unhealthy => f.write_str("unhealthy"),
        }
    }
}

/// Current status of a tracked service.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    /// Service name.
    pub name: String,
    /// Current health level.
    pub health: ServiceHealth,
    /// Monotonic time of the last status check.
    #[serde(skip)]
    pub last_check: Option<Instant>,
    /// Monotonic time of the last health change.
    #[serde(skip)]
    pub last_change: Option<Instant>,
    /// Error message, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Tracks service health and feature flags for graceful degradation.
///
/// Thread-safe via [`parking_lot::RwLock`].
#[derive(Clone)]
pub struct DegradationManager {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    services: HashMap<String, ServiceStatus>,
    features: HashMap<String, bool>,
}

impl DegradationManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                services: HashMap::new(),
                features: HashMap::new(),
            })),
        }
    }

    /// Set the health status for a named service.
    pub fn update_service(
        &self,
        name: &str,
        health: ServiceHealth,
        error: Option<String>,
    ) {
        let mut inner = self.inner.write();
        let now = Instant::now();
        let existing = inner.services.get(name);

        let last_change = match existing {
            Some(s) if s.health == health => s.last_change,
            _ => Some(now),
        };

        inner.services.insert(
            name.to_string(),
            ServiceStatus {
                name: name.to_string(),
                health,
                last_check: Some(now),
                last_change,
                error,
            },
        );
    }

    /// Return the status of a named service.
    /// Returns a default Healthy status if the service is not tracked.
    pub fn service_status(&self, name: &str) -> ServiceStatus {
        let inner = self.inner.read();
        inner
            .services
            .get(name)
            .cloned()
            .unwrap_or(ServiceStatus {
                name: name.to_string(),
                health: ServiceHealth::Healthy,
                last_check: None,
                last_change: None,
                error: None,
            })
    }

    /// Return a snapshot of all tracked service statuses.
    pub fn all_statuses(&self) -> HashMap<String, ServiceStatus> {
        self.inner.read().services.clone()
    }

    /// Enable or disable a feature flag.
    pub fn set_feature(&self, name: &str, enabled: bool) {
        self.inner.write().features.insert(name.to_string(), enabled);
    }

    /// Return whether a feature flag is enabled.
    /// Returns `false` for unknown features.
    pub fn feature_enabled(&self, name: &str) -> bool {
        self.inner.read().features.get(name).copied().unwrap_or(false)
    }

    /// Return `true` only if all tracked services are [`Healthy`](ServiceHealth::Healthy).
    pub fn is_healthy(&self) -> bool {
        self.inner
            .read()
            .services
            .values()
            .all(|s| s.health == ServiceHealth::Healthy)
    }

    /// Return a callback compatible with `CbConfig::on_state_change`.
    ///
    /// Maps circuit breaker states to service health:
    /// - `Closed`   → `Healthy`
    /// - `HalfOpen` → `Degraded`
    /// - `Open`     → `Unhealthy`
    pub fn on_cb_state_change(
        &self,
        service_name: &str,
    ) -> Arc<dyn Fn(CbState, CbState) + Send + Sync> {
        let dm = self.clone();
        let name = service_name.to_string();
        Arc::new(move |_from, to| match to {
            CbState::Closed => dm.update_service(&name, ServiceHealth::Healthy, None),
            CbState::HalfOpen => dm.update_service(&name, ServiceHealth::Degraded, None),
            CbState::Open => dm.update_service(&name, ServiceHealth::Unhealthy, None),
        })
    }
}

impl Default for DegradationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manager_is_healthy() {
        let dm = DegradationManager::new();
        assert!(dm.is_healthy());
    }

    #[test]
    fn update_and_query_service() {
        let dm = DegradationManager::new();
        dm.update_service("db", ServiceHealth::Healthy, None);

        let s = dm.service_status("db");
        assert_eq!(s.health, ServiceHealth::Healthy);
        assert!(s.error.is_none());
    }

    #[test]
    fn unhealthy_service_makes_aggregate_unhealthy() {
        let dm = DegradationManager::new();
        dm.update_service("db", ServiceHealth::Healthy, None);
        dm.update_service("redis", ServiceHealth::Unhealthy, Some("connection refused".into()));

        assert!(!dm.is_healthy());
        let s = dm.service_status("redis");
        assert_eq!(s.health, ServiceHealth::Unhealthy);
        assert_eq!(s.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn feature_flags() {
        let dm = DegradationManager::new();
        assert!(!dm.feature_enabled("dark-launch"));

        dm.set_feature("dark-launch", true);
        assert!(dm.feature_enabled("dark-launch"));

        dm.set_feature("dark-launch", false);
        assert!(!dm.feature_enabled("dark-launch"));
    }

    #[test]
    fn cb_state_change_callback() {
        let dm = DegradationManager::new();
        let cb = dm.on_cb_state_change("api-gateway");

        // Simulate: Closed → Open
        cb(CbState::Closed, CbState::Open);
        assert_eq!(
            dm.service_status("api-gateway").health,
            ServiceHealth::Unhealthy
        );

        // Simulate: Open → HalfOpen
        cb(CbState::Open, CbState::HalfOpen);
        assert_eq!(
            dm.service_status("api-gateway").health,
            ServiceHealth::Degraded
        );

        // Simulate: HalfOpen → Closed
        cb(CbState::HalfOpen, CbState::Closed);
        assert_eq!(
            dm.service_status("api-gateway").health,
            ServiceHealth::Healthy
        );
    }

    #[test]
    fn last_change_preserved_when_health_unchanged() {
        let dm = DegradationManager::new();
        dm.update_service("svc", ServiceHealth::Healthy, None);

        let first_change = dm.service_status("svc").last_change;

        // Update with same health
        dm.update_service("svc", ServiceHealth::Healthy, None);
        let second_change = dm.service_status("svc").last_change;

        assert_eq!(first_change, second_change);
    }

    #[test]
    fn all_statuses_snapshot() {
        let dm = DegradationManager::new();
        dm.update_service("a", ServiceHealth::Healthy, None);
        dm.update_service("b", ServiceHealth::Degraded, None);

        let snap = dm.all_statuses();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap["a"].health, ServiceHealth::Healthy);
        assert_eq!(snap["b"].health, ServiceHealth::Degraded);
    }

    #[test]
    fn unknown_service_returns_default() {
        let dm = DegradationManager::new();
        let s = dm.service_status("unknown");
        assert_eq!(s.health, ServiceHealth::Healthy);
        assert_eq!(s.name, "unknown");
    }

    #[test]
    fn service_health_display() {
        assert_eq!(ServiceHealth::Healthy.to_string(), "healthy");
        assert_eq!(ServiceHealth::Degraded.to_string(), "degraded");
        assert_eq!(ServiceHealth::Unhealthy.to_string(), "unhealthy");
    }
}

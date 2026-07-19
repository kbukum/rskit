//! Service health tracking for aggregate component monitoring.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// Health status of a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum HealthStatus {
    /// Component is operating normally.
    Healthy,
    /// Component is operating with reduced capability.
    Degraded,
    /// Component is not operational.
    Unhealthy,
}

/// Health information for a single component.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentHealth {
    /// Name of the component.
    pub name: String,
    /// Current health status.
    pub status: HealthStatus,
    /// Optional human-readable message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Tracks aggregate health of service components.
///
/// Thread-safe via `Arc<RwLock<...>>`. Register components and update their status;
/// query overall health at any time.
///
/// Mirrors Go's `observability.ServiceHealth`.
#[derive(Clone)]
pub struct ServiceHealth {
    service: String,
    version: String,
    components: Arc<RwLock<HashMap<String, ComponentHealth>>>,
}

impl ServiceHealth {
    /// Create a new service health tracker.
    pub fn new(service: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            version: version.into(),
            components: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Service name.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Service version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Register a component (initially healthy).
    pub fn register(&self, name: impl Into<String>) {
        let name = name.into();
        let mut map = self.components.write();
        map.insert(
            name.clone(),
            ComponentHealth {
                name,
                status: HealthStatus::Healthy,
                message: None,
            },
        );
    }

    /// Update the health status of a registered component.
    pub fn update(&self, name: impl Into<String>, status: HealthStatus, message: Option<String>) {
        let name = name.into();
        let mut map = self.components.write();
        map.insert(
            name.clone(),
            ComponentHealth {
                name,
                status,
                message,
            },
        );
    }

    /// Returns `true` if all components are healthy.
    pub fn is_healthy(&self) -> bool {
        let map = self.components.read();
        map.values().all(|c| c.status == HealthStatus::Healthy)
    }

    /// Returns a snapshot of all component health states.
    pub fn status(&self) -> HashMap<String, ComponentHealth> {
        self.components.read().clone()
    }

    /// Returns the worst health status across all components.
    pub fn overall_status(&self) -> HealthStatus {
        let map = self.components.read();
        if map.is_empty() {
            return HealthStatus::Healthy;
        }
        if map.values().any(|c| c.status == HealthStatus::Unhealthy) {
            return HealthStatus::Unhealthy;
        }
        if map.values().any(|c| c.status == HealthStatus::Degraded) {
            return HealthStatus::Degraded;
        }
        HealthStatus::Healthy
    }
}

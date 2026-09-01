//! Service health tracking for aggregate component monitoring.

use std::collections::{BTreeMap, HashMap};
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComponentHealth {
    /// Name of the component.
    pub name: String,
    /// Current health status.
    pub status: HealthStatus,
    /// Optional human-readable message.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
    /// Optional structured detail attributes (latency, pool size, and so on).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

/// Serializable snapshot of a service and its component health.
///
/// This is the shared cross-cutting health document that server and discovery layers reuse:
/// the status vocabulary is exactly `healthy`/`degraded`/`unhealthy`, and empty `version` or
/// `components` are omitted so the document stays minimal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceHealthReport {
    /// Logical service name.
    pub service: String,
    /// Worst status across all components.
    pub status: HealthStatus,
    /// Service version, omitted when empty.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub version: String,
    /// Component health snapshots, sorted by name and omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub components: Vec<ComponentHealth>,
}

/// Worst status across a set of component health snapshots.
///
/// An empty set is treated as [`HealthStatus::Healthy`].
fn worst_status(components: &[ComponentHealth]) -> HealthStatus {
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

/// Tracks aggregate health of service components.
///
/// Thread-safe via `Arc<RwLock<...>>`. Register components and update their status;
/// query overall health at any time, or snapshot a serializable [`ServiceHealthReport`].
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
                details: BTreeMap::new(),
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
                details: BTreeMap::new(),
            },
        );
    }

    /// Record a complete [`ComponentHealth`], including structured `details`.
    ///
    /// Unlike [`register`](Self::register) and [`update`](Self::update), which force `details`
    /// empty, this stores the supplied value verbatim so reports carry the structured detail
    /// attributes. The component is keyed by [`ComponentHealth::name`].
    pub fn set(&self, health: ComponentHealth) {
        self.components.write().insert(health.name.clone(), health);
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
        let components: Vec<ComponentHealth> = map.values().cloned().collect();
        worst_status(&components)
    }

    /// Snapshot the current health as a serializable [`ServiceHealthReport`].
    ///
    /// Components are sorted by name so the document is deterministic.
    pub fn report(&self) -> ServiceHealthReport {
        let mut components: Vec<ComponentHealth> =
            self.components.read().values().cloned().collect();
        components.sort_by(|a, b| a.name.cmp(&b.name));
        ServiceHealthReport {
            service: self.service.clone(),
            status: worst_status(&components),
            version: self.version.clone(),
            components,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthStatus, ServiceHealth, ServiceHealthReport};

    #[test]
    fn report_snapshots_sorted_components_and_worst_status() {
        let health = ServiceHealth::new("orders", "1.2.3");
        health.register("db");
        health.register("cache");
        health.update("cache", HealthStatus::Degraded, Some("slow".to_string()));

        let report = health.report();
        assert_eq!(report.service, "orders");
        assert_eq!(report.version, "1.2.3");
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.components.len(), 2);
        // Sorted by name: cache before db.
        assert_eq!(report.components[0].name, "cache");
        assert_eq!(report.components[0].status, HealthStatus::Degraded);
        assert_eq!(report.components[1].name, "db");
    }

    #[test]
    fn report_serializes_only_lowercase_status_vocabulary() {
        let health = ServiceHealth::new("svc", "");
        health.register("worker");
        health.update("worker", HealthStatus::Unhealthy, None);

        let json = serde_json::to_string(&health.report()).unwrap();
        assert!(json.contains("\"status\":\"unhealthy\""));
        // Empty version is omitted.
        assert!(!json.contains("\"version\""));
    }

    #[test]
    fn report_round_trips_through_serde() {
        let health = ServiceHealth::new("svc", "9.9.9");
        health.register("db");
        let report = health.report();
        let json = serde_json::to_string(&report).unwrap();
        let decoded: ServiceHealthReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, report);
    }

    #[test]
    fn set_carries_structured_details_into_report() {
        use super::ComponentHealth;
        use std::collections::BTreeMap;

        let health = ServiceHealth::new("svc", "1.0");
        let mut details = BTreeMap::new();
        details.insert("pool_size".to_string(), "8".to_string());
        details.insert("latency_ms".to_string(), "12".to_string());
        health.set(ComponentHealth {
            name: "db".to_string(),
            status: HealthStatus::Degraded,
            message: Some("slow".to_string()),
            details,
        });

        let report = health.report();
        assert_eq!(report.components.len(), 1);
        assert_eq!(report.components[0].details.get("pool_size").unwrap(), "8");

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"details\""));
        assert!(json.contains("\"pool_size\":\"8\""));
    }

    #[test]
    fn report_matches_cross_kit_golden_fixture() {
        let health = ServiceHealth::new("orders", "1.2.3");
        health.register("db");
        health.register("cache");
        health.update("cache", HealthStatus::Degraded, Some("slow".to_string()));

        let actual = serde_json::to_string_pretty(&health.report()).unwrap();
        let expected =
            include_str!("../tests/fixtures/cross-kit/observability/service-health.json");
        assert_eq!(format!("{actual}\n"), expected);
    }
}

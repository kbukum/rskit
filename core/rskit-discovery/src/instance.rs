use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Tri-state health of a discovered service instance.
///
/// A registry that has not yet probed an instance reports [`HealthState::Unknown`] rather than
/// guessing. Discovery listings (for example [`Discovery::resolve`](crate::Discovery::resolve))
/// return every registered instance regardless of health; callers are responsible for routing only
/// to healthy instances. Use [`ServiceInstance::is_healthy`] to filter a candidate set, or
/// [`crate::resolve_addr`], which already selects the first [`HealthState::Healthy`] instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum HealthState {
    /// Health has not been determined yet.
    #[default]
    Unknown,
    /// Instance is passing its health checks.
    Healthy,
    /// Instance is failing its health checks.
    Unhealthy,
}

/// A single instance of a service available for discovery.
///
/// Serializes to snake_case JSON shared across kits: `id`, `name`, `address`, `port`,
/// `protocol`, `tags`, `metadata`, `health`, `weight`, and `last_seen`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Unique instance identifier (e.g. UUID).
    pub id: String,
    /// Logical service name (e.g. `"payment-service"`).
    pub name: String,
    /// Host or IP address.
    pub address: String,
    /// Port the service is listening on.
    pub port: u16,
    /// Application protocol (e.g. `"grpc"` or `"http"`), empty when unspecified.
    #[serde(default)]
    pub protocol: String,
    /// Freeform tags for filtering (e.g. `["canary", "us-east-1"]`).
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
    /// Tri-state health of the instance. Defaults to [`HealthState::Unknown`] when omitted.
    #[serde(default)]
    pub health: HealthState,
    /// Relative load-balancing weight. Zero is treated as one by weighted balancers.
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// RFC 3339 UTC timestamp of the last successful observation, or `None` when never seen.
    #[serde(default)]
    pub last_seen: Option<String>,
}

fn default_weight() -> u32 {
    1
}

impl ServiceInstance {
    /// Create an instance with unknown health, empty protocol, unit weight, and no `last_seen`.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        address: impl Into<String>,
        port: u16,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            address: address.into(),
            port,
            protocol: String::new(),
            tags: Vec::new(),
            metadata: HashMap::new(),
            health: HealthState::Unknown,
            weight: default_weight(),
            last_seen: None,
        }
    }

    /// Returns `"address:port"`.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }

    /// Returns `true` only when the instance is known [`HealthState::Healthy`].
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.health == HealthState::Healthy
    }

    /// Set the instance health state.
    #[must_use]
    pub fn with_health(mut self, health: HealthState) -> Self {
        self.health = health;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_optional_fields_deserialize_to_defaults() {
        let instance: ServiceInstance = serde_json::from_str(
            r#"{
                "id":"a",
                "name":"svc",
                "address":"127.0.0.1",
                "port":8080,
                "tags":[],
                "metadata":{}
            }"#,
        )
        .unwrap();

        assert_eq!(instance.weight, 1);
        assert_eq!(instance.health, HealthState::Unknown);
        assert_eq!(instance.protocol, "");
        assert_eq!(instance.last_seen, None);
        assert!(!instance.is_healthy());
    }

    #[test]
    fn tri_state_health_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(HealthState::Unknown).unwrap(),
            serde_json::json!("unknown")
        );
        assert_eq!(
            serde_json::to_value(HealthState::Healthy).unwrap(),
            serde_json::json!("healthy")
        );
        assert_eq!(
            serde_json::to_value(HealthState::Unhealthy).unwrap(),
            serde_json::json!("unhealthy")
        );
    }

    #[test]
    fn instance_matches_cross_kit_golden_fixture() {
        let mut metadata = HashMap::new();
        metadata.insert("zone".to_string(), "us-east-1a".to_string());
        let instance = ServiceInstance {
            id: "payment-1".to_string(),
            name: "payment-service".to_string(),
            address: "10.0.0.5".to_string(),
            port: 8080,
            protocol: "grpc".to_string(),
            tags: vec!["canary".to_string()],
            metadata,
            health: HealthState::Healthy,
            weight: 1,
            last_seen: Some("2026-03-05T10:00:00Z".to_string()),
        };

        let actual = serde_json::to_string_pretty(&instance).unwrap();
        let expected = include_str!("../tests/fixtures/cross-kit/discovery/service-instance.json");
        assert_eq!(format!("{actual}\n"), expected);
    }
}

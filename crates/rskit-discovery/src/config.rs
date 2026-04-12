//! Service discovery configuration.
//!
//! Mirrors gokit's `discovery.Config` — all three kits use the same config
//! shape so services are structurally identical regardless of language.

use std::collections::HashMap;

use serde::Deserialize;

/// Top-level discovery configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Whether discovery is active.
    pub enabled: bool,
    /// Discovery backend: `"consul"` or `"static"`.
    pub provider: String,
    /// Self-registration settings.
    pub registration: RegistrationConfig,
    /// Health check settings for registered services.
    pub health: HealthConfig,
    /// How long discovered endpoints are cached (e.g. `"30s"`).
    pub cache_ttl: String,
    /// Remote services this application depends on.
    #[serde(default)]
    pub services: Vec<DiscoveredService>,
    /// Static endpoint fallback (used by the static provider or when consul is unavailable).
    #[serde(default)]
    pub static_endpoints: Vec<StaticEndpoint>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "static".to_string(),
            registration: RegistrationConfig::default(),
            health: HealthConfig::default(),
            cache_ttl: "30s".to_string(),
            services: Vec::new(),
            static_endpoints: Vec::new(),
        }
    }
}

impl DiscoveryConfig {
    /// Apply sensible defaults to zero-valued fields.
    pub fn apply_defaults(&mut self) {
        if self.provider.is_empty() {
            self.provider = "static".to_string();
        }
        self.registration.apply_defaults();
        self.health.apply_defaults();
    }

    /// Validate that required fields are present.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.registration.enabled {
            if self.registration.service_name.is_empty() {
                return Err("discovery.registration.service_name is required".into());
            }
            if self.registration.service_port == 0 {
                return Err("discovery.registration.service_port must be > 0".into());
            }
        }
        Ok(())
    }

    /// Build a [`ServiceInstance`](crate::instance::ServiceInstance) from the registration config.
    pub fn build_instance(&self) -> crate::instance::ServiceInstance {
        let reg = &self.registration;
        let id = if reg.service_id.is_empty() {
            reg.service_name.clone()
        } else {
            reg.service_id.clone()
        };
        crate::instance::ServiceInstance {
            id,
            name: reg.service_name.clone(),
            address: reg.service_address.clone(),
            port: reg.service_port,
            healthy: true,
            tags: reg.tags.clone(),
            metadata: reg.metadata.clone(),
        }
    }
}

/// Self-registration settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RegistrationConfig {
    /// Toggle self-registration.
    pub enabled: bool,
    /// Name used when registering.
    pub service_name: String,
    /// Unique instance ID; defaults to `service_name` if empty.
    pub service_id: String,
    /// Address advertised to other services.
    pub service_address: String,
    /// Port advertised to other services.
    pub service_port: u16,
    /// Metadata tags attached to the registration.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl RegistrationConfig {
    /// Apply defaults to zero-valued fields.
    pub fn apply_defaults(&mut self) {
        if self.service_id.is_empty() && !self.service_name.is_empty() {
            self.service_id = self.service_name.clone();
        }
    }
}

/// Health check configuration for registered services.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HealthConfig {
    /// Toggle health checks.
    pub enabled: bool,
    /// Health check type: `"http"`, `"grpc"`, `"tcp"`, or `"ttl"`.
    #[serde(rename = "type")]
    pub check_type: String,
    /// HTTP path for health checks.
    pub path: String,
    /// How often health is polled (e.g. `"10s"`).
    pub interval: String,
    /// Timeout for a single health check.
    pub timeout: String,
    /// Remove service after being critical for this duration.
    pub deregister_after: String,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_type: "http".to_string(),
            path: "/health".to_string(),
            interval: "10s".to_string(),
            timeout: "5s".to_string(),
            deregister_after: "1m".to_string(),
        }
    }
}

impl HealthConfig {
    /// Apply defaults to zero-valued fields.
    pub fn apply_defaults(&mut self) {
        if self.check_type.is_empty() {
            self.check_type = "http".to_string();
        }
        if self.path.is_empty() {
            self.path = "/health".to_string();
        }
        if self.interval.is_empty() {
            self.interval = "10s".to_string();
        }
        if self.timeout.is_empty() {
            self.timeout = "5s".to_string();
        }
        if self.deregister_after.is_empty() {
            self.deregister_after = "1m".to_string();
        }
    }
}

/// A remote service this application depends on.
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveredService {
    /// Logical service name (e.g. `"ssm-ingestion"`).
    pub name: String,
    /// Protocol: `"grpc"` or `"http"`.
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

/// A statically configured endpoint (fallback or static provider).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StaticEndpoint {
    /// Logical service name.
    pub name: String,
    /// Host or IP address.
    pub address: String,
    /// Port.
    pub port: u16,
    /// Protocol: `"grpc"` or `"http"`.
    pub protocol: String,
    /// Tags for filtering.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Weight for load balancing.
    pub weight: u32,
    /// Whether this endpoint is healthy.
    pub healthy: bool,
}

impl Default for StaticEndpoint {
    fn default() -> Self {
        Self {
            name: String::new(),
            address: String::new(),
            port: 0,
            protocol: "grpc".to_string(),
            tags: Vec::new(),
            metadata: HashMap::new(),
            weight: 1,
            healthy: true,
        }
    }
}

fn default_protocol() -> String {
    "grpc".to_string()
}

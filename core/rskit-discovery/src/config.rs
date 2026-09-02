//! Service discovery configuration.
//!
//! A single provider-agnostic config shape so services are structurally identical regardless of backend.

use std::collections::HashMap;

use rskit_errors::{AppError, AppResult};
use rskit_util::time::parse_duration;
use serde::Deserialize;

/// Top-level discovery configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Whether discovery is active.
    pub enabled: bool,
    /// Discovery backend: `"consul"`, `"static"`, etc.
    pub provider: String,
    /// Provider address (e.g. `"localhost:8500"` for Consul). Generic —
    /// every remote provider needs an address.
    pub addr: String,
    /// URI scheme for the provider connection (`"http"`, `"https"`).
    pub scheme: String,
    /// Auth token for the discovery provider.
    pub token: String,
    /// Self-registration settings.
    pub registration: RegistrationConfig,
    /// Health check settings for registered services.
    pub health: HealthConfig,
    /// How long discovered endpoints are cached (e.g. `"30s"`).
    pub cache_ttl: String,
    /// Remote services this application depends on.
    #[serde(default)]
    pub services: Vec<DiscoveredService>,
    /// Static endpoint fallback (used by the static provider or when the backend is unavailable).
    #[serde(default)]
    pub static_endpoints: Vec<StaticEndpoint>,
    /// Exotic provider-specific settings (e.g. datacenter, TLS, pool for Consul).
    /// Generic fields like addr/scheme/token are on the config directly.
    #[serde(default)]
    pub provider_options: toml::Table,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "static".to_string(),
            addr: String::new(),
            scheme: "http".to_string(),
            token: String::new(),
            registration: RegistrationConfig::default(),
            health: HealthConfig::default(),
            cache_ttl: "30s".to_string(),
            services: Vec::new(),
            static_endpoints: Vec::new(),
            provider_options: toml::Table::new(),
        }
    }
}

impl DiscoveryConfig {
    /// Apply sensible defaults to zero-valued fields.
    pub fn apply_defaults(&mut self) {
        if self.provider.is_empty() {
            self.provider = "static".to_string();
        }
        if self.scheme.is_empty() {
            self.scheme = "http".to_string();
        }
        self.registration.apply_defaults();
        self.health.apply_defaults();
    }

    /// Validate that required fields are present.
    pub fn validate(&self) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.registration.enabled {
            if self.registration.service_name.is_empty() {
                return Err(AppError::invalid_input(
                    "discovery.registration.service_name",
                    "service name is required",
                ));
            }
            if self.registration.service_port == 0 {
                return Err(AppError::invalid_input(
                    "discovery.registration.service_port",
                    "service port must be greater than zero",
                ));
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
            protocol: String::new(),
            health: crate::instance::HealthState::Healthy,
            weight: 1,
            tags: reg.tags.clone(),
            metadata: reg.metadata.clone(),
            last_seen: None,
        }
    }
}

/// Self-registration settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegistrationConfig {
    /// Toggle self-registration.
    pub enabled: bool,
    /// When true (the default), the service will retry with backoff
    /// and fail to start if registration cannot be completed — appropriate for staging/production.
    /// When false, logs a warning and continues in degraded mode — convenient for local development.
    pub required: bool,
    /// Number of registration retries before giving up. Defaults to 3.
    pub max_retries: u32,
    /// Base interval between retries (e.g. `"2s"`). Doubles each retry.
    pub retry_interval: String,
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

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            required: true,
            max_retries: 3,
            retry_interval: "2s".to_string(),
            service_name: String::new(),
            service_id: String::new(),
            service_address: String::new(),
            service_port: 0,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

impl RegistrationConfig {
    /// Apply defaults to zero-valued fields.
    pub fn apply_defaults(&mut self) {
        if self.service_id.is_empty() && !self.service_name.is_empty() {
            self.service_id = self.service_name.clone();
        }
        if self.max_retries == 0 {
            self.max_retries = 3;
        }
        if self.retry_interval.is_empty() {
            self.retry_interval = "2s".to_string();
        }
    }

    /// Parse the retry interval as a [`Duration`](std::time::Duration).
    pub fn retry_duration(&self) -> AppResult<std::time::Duration> {
        parse_duration(&self.retry_interval).ok_or_else(|| {
            AppError::invalid_input(
                "discovery.registration.retry_interval",
                format!("invalid duration '{}'", self.retry_interval),
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn discovery_defaults_are_static_and_disabled() {
        let cfg = DiscoveryConfig::default();

        assert!(!cfg.enabled);
        assert_eq!(cfg.provider, "static");
        assert_eq!(cfg.scheme, "http");
        assert_eq!(cfg.cache_ttl, "30s");
        assert!(cfg.services.is_empty());
        assert!(cfg.static_endpoints.is_empty());
    }

    #[test]
    fn apply_defaults_fills_nested_zero_values_without_overwriting_explicit_values() {
        let mut cfg = DiscoveryConfig {
            provider: String::new(),
            scheme: String::new(),
            registration: RegistrationConfig {
                service_name: "svc".to_string(),
                max_retries: 0,
                retry_interval: String::new(),
                ..Default::default()
            },
            health: HealthConfig {
                check_type: String::new(),
                path: String::new(),
                interval: String::new(),
                timeout: String::new(),
                deregister_after: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };

        cfg.apply_defaults();

        assert_eq!(cfg.provider, "static");
        assert_eq!(cfg.scheme, "http");
        assert_eq!(cfg.registration.service_id, "svc");
        assert_eq!(cfg.registration.max_retries, 3);
        assert_eq!(cfg.registration.retry_interval, "2s");
        assert_eq!(cfg.health.check_type, "http");
        assert_eq!(cfg.health.path, "/health");
        assert_eq!(cfg.health.interval, "10s");
        assert_eq!(cfg.health.timeout, "5s");
        assert_eq!(cfg.health.deregister_after, "1m");
    }

    #[test]
    fn validate_ignores_disabled_registration_but_rejects_missing_enabled_fields() {
        let disabled = DiscoveryConfig {
            enabled: false,
            registration: RegistrationConfig {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(disabled.validate().is_ok());

        let missing_name = DiscoveryConfig {
            enabled: true,
            registration: RegistrationConfig {
                enabled: true,
                service_port: 8080,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            missing_name
                .validate()
                .unwrap_err()
                .to_string()
                .contains("service name")
        );

        let missing_port = DiscoveryConfig {
            enabled: true,
            registration: RegistrationConfig {
                enabled: true,
                service_name: "svc".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            missing_port
                .validate()
                .unwrap_err()
                .to_string()
                .contains("port")
        );
    }

    #[test]
    fn build_instance_uses_service_name_as_default_id_and_preserves_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("zone".to_string(), "a".to_string());
        let cfg = DiscoveryConfig {
            registration: RegistrationConfig {
                service_name: "api".to_string(),
                service_address: "127.0.0.1".to_string(),
                service_port: 8080,
                tags: vec!["blue".to_string()],
                metadata: metadata.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let instance = cfg.build_instance();

        assert_eq!(instance.id, "api");
        assert_eq!(instance.name, "api");
        assert_eq!(instance.address, "127.0.0.1");
        assert_eq!(instance.port, 8080);
        assert_eq!(instance.tags, vec!["blue"]);
        assert_eq!(instance.metadata, metadata);
    }

    #[test]
    fn retry_duration_accepts_valid_values_and_rejects_invalid_values() {
        let valid = RegistrationConfig {
            retry_interval: "250ms".to_string(),
            ..Default::default()
        };
        assert_eq!(
            valid.retry_duration().unwrap(),
            std::time::Duration::from_millis(250)
        );

        let invalid = RegistrationConfig {
            retry_interval: "soon".to_string(),
            ..Default::default()
        };
        assert!(
            invalid
                .retry_duration()
                .unwrap_err()
                .to_string()
                .contains("invalid duration")
        );
    }

    #[test]
    fn serde_defaults_fill_protocol_and_static_endpoint_fields() {
        #[derive(Deserialize)]
        struct Wrapper {
            services: Vec<DiscoveredService>,
            endpoint: StaticEndpoint,
        }

        let parsed: Wrapper = toml::from_str(
            r#"
            [[services]]
            name = "users"

            [endpoint]
            name = "users"
            address = "127.0.0.1"
            "#,
        )
        .unwrap();

        assert_eq!(parsed.services[0].protocol, "grpc");
        assert_eq!(parsed.endpoint.port, 0);
        assert_eq!(parsed.endpoint.protocol, "grpc");
        assert_eq!(parsed.endpoint.weight, 1);
        assert!(parsed.endpoint.healthy);
    }
}

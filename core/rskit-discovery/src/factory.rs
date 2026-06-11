//! Explicit provider factory registry.

use std::collections::HashMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::DiscoveryConfig;
use crate::traits::{Discovery, Registry};

/// A pair of `(Registry, Discovery)` returned by a provider factory.
pub type ProviderPair = (Arc<dyn Registry>, Arc<dyn Discovery>);

/// Factory function type: creates a provider pair from a discovery config.
pub type ProviderFactory = Box<dyn Fn(&DiscoveryConfig) -> AppResult<ProviderPair> + Send + Sync>;

/// Explicit discovery provider registry.
#[derive(Default)]
pub struct DiscoveryRegistry {
    factories: HashMap<String, ProviderFactory>,
}

impl DiscoveryRegistry {
    /// Create an empty provider registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with built-in providers registered.
    #[must_use]
    pub fn builtins() -> Self {
        let mut registry = Self::new();
        registry.register(
            "static",
            Box::new(|config| {
                let mem = Arc::new(crate::memory::InMemoryDiscovery::new());
                for ep in &config.static_endpoints {
                    let inst = crate::instance::ServiceInstance {
                        id: format!("{}-{}:{}", ep.name, ep.address, ep.port),
                        name: ep.name.clone(),
                        address: ep.address.clone(),
                        port: ep.port,
                        healthy: ep.healthy,
                        weight: ep.weight,
                        tags: ep.tags.clone(),
                        metadata: ep.metadata.clone(),
                    };
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(mem.add(&ep.name, inst))
                    });
                }
                let arc: Arc<crate::memory::InMemoryDiscovery> = mem;
                Ok((arc.clone(), arc))
            }),
        );

        #[cfg(feature = "consul")]
        registry.register(
            "consul",
            Box::new(|config| {
                let addr = if config.addr.is_empty() {
                    "localhost:8500"
                } else {
                    &config.addr
                };
                let token = if config.token.is_empty() {
                    None
                } else {
                    Some(config.token.clone())
                };
                let consul = Arc::new(crate::consul::ConsulDiscovery::new(addr, token)?);
                Ok((consul.clone(), consul))
            }),
        );

        registry
    }

    /// Register a provider factory under the given name.
    pub fn register(&mut self, name: impl Into<String>, factory: ProviderFactory) {
        let name = name.into();
        tracing::debug!(provider = %name, "registered discovery provider factory");
        self.factories.insert(name, factory);
    }

    /// Create a provider pair for the provider specified by the config.
    pub fn create(&self, config: &DiscoveryConfig) -> AppResult<ProviderPair> {
        let factory = self.factories.get(&config.provider).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "unsupported discovery provider {:?} (registered: {:?})",
                    config.provider,
                    self.factories.keys().collect::<Vec<_>>()
                ),
            )
        })?;
        factory(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_returns_error() {
        let registry = DiscoveryRegistry::builtins();
        let cfg = DiscoveryConfig {
            provider: "unknown-provider".to_string(),
            ..Default::default()
        };
        let result = registry.create(&cfg);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn static_provider_creates_successfully() {
        let registry = DiscoveryRegistry::builtins();
        let cfg = DiscoveryConfig {
            provider: "static".to_string(),
            ..Default::default()
        };
        let result = registry.create(&cfg);
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn static_provider_registers_configured_endpoints() {
        let registry = DiscoveryRegistry::builtins();
        let cfg = DiscoveryConfig {
            provider: "static".to_string(),
            static_endpoints: vec![crate::config::StaticEndpoint {
                name: "users".to_string(),
                address: "127.0.0.1".to_string(),
                port: 8080,
                protocol: "grpc".to_string(),
                tags: vec!["blue".to_string()],
                metadata: [("zone".to_string(), "a".to_string())]
                    .into_iter()
                    .collect(),
                weight: 3,
                healthy: false,
            }],
            ..Default::default()
        };

        let (_reg, disc) = registry.create(&cfg).unwrap();
        let instances = disc.resolve("users").await.unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "users-127.0.0.1:8080");
        assert_eq!(instances[0].tags, vec!["blue"]);
        assert_eq!(
            instances[0].metadata.get("zone").map(String::as_str),
            Some("a")
        );
        assert_eq!(instances[0].weight, 3);
        assert!(!instances[0].healthy);
    }

    #[test]
    fn explicit_factory_registration_can_override_provider_name() {
        let mut registry = DiscoveryRegistry::new();
        registry.register(
            "custom",
            Box::new(|_config| {
                let mem = Arc::new(crate::memory::InMemoryDiscovery::new());
                Ok((mem.clone(), mem))
            }),
        );

        let cfg = DiscoveryConfig {
            provider: "custom".to_string(),
            ..Default::default()
        };

        assert!(registry.create(&cfg).is_ok());
    }

    #[cfg(feature = "consul")]
    #[test]
    fn builtin_consul_factory_uses_default_address_and_optional_token() {
        let registry = DiscoveryRegistry::builtins();
        let cfg = DiscoveryConfig {
            provider: "consul".to_string(),
            token: "secret".to_string(),
            ..Default::default()
        };

        assert!(registry.create(&cfg).is_ok());
    }
}

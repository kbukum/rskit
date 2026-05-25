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
}

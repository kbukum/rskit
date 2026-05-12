//! Provider factory registry.
//!
//! Provider implementations register themselves here so the
//! [`DiscoveryComponent`](crate::component::DiscoveryComponent) can
//! create them from a [`DiscoveryConfig`](crate::config::DiscoveryConfig)
//! without importing provider-specific types.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::DiscoveryConfig;
use crate::traits::{Discovery, Registry};

/// A pair of `(Registry, Discovery)` returned by a provider factory.
pub type ProviderPair = (std::sync::Arc<dyn Registry>, std::sync::Arc<dyn Discovery>);

/// Factory function type: creates a provider pair from a discovery config.
pub type ProviderFactory = Box<dyn Fn(&DiscoveryConfig) -> AppResult<ProviderPair> + Send + Sync>;

// Global factory registry.
static FACTORIES: OnceLock<Mutex<HashMap<String, ProviderFactory>>> = OnceLock::new();

fn factories() -> &'static Mutex<HashMap<String, ProviderFactory>> {
    FACTORIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a provider factory under the given name (e.g. `"consul"`).
///
/// Typically called once at process start before any component is created.
pub fn register_provider(name: impl Into<String>, factory: ProviderFactory) {
    let name = name.into();
    tracing::debug!(provider = %name, "Registered discovery provider factory");
    factories().lock().insert(name, factory);
}

/// Create a `(Registry, Discovery)` pair for the provider specified in `config.provider`.
pub fn create_provider(config: &DiscoveryConfig) -> AppResult<ProviderPair> {
    let map = factories().lock();
    let factory = map.get(&config.provider).ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "unsupported discovery provider {:?} (registered: {:?})",
                config.provider,
                map.keys().collect::<Vec<_>>()
            ),
        )
    })?;
    factory(config)
}

/// Register all built-in providers.
///
/// Called automatically by [`DiscoveryComponent::start()`](crate::component::DiscoveryComponent).
pub fn init_builtin() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Static / in-memory provider — always available.
        register_provider(
            "static",
            Box::new(|config| {
                let mem = std::sync::Arc::new(crate::memory::InMemoryDiscovery::new());
                // Pre-populate from static_endpoints.
                for ep in &config.static_endpoints {
                    let inst = crate::instance::ServiceInstance {
                        id: format!("{}-{}:{}", ep.name, ep.address, ep.port),
                        name: ep.name.clone(),
                        address: ep.address.clone(),
                        port: ep.port,
                        healthy: ep.healthy,
                        tags: ep.tags.clone(),
                        metadata: ep.metadata.clone(),
                    };
                    // InMemoryDiscovery::add is async; block_in_place since we are
                    // inside a sync factory called during component start.
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(mem.add(&ep.name, inst))
                    });
                }
                let arc: std::sync::Arc<crate::memory::InMemoryDiscovery> = mem;
                Ok((arc.clone(), arc))
            }),
        );

        // Consul provider — only when the feature is enabled.
        #[cfg(feature = "consul")]
        register_provider(
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
                let consul = std::sync::Arc::new(crate::consul::ConsulDiscovery::new(addr, token));
                Ok((consul.clone(), consul))
            }),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_returns_error() {
        init_builtin();
        let cfg = DiscoveryConfig {
            provider: "unknown-provider".to_string(),
            ..Default::default()
        };
        let result = create_provider(&cfg);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn static_provider_creates_successfully() {
        init_builtin();
        let cfg = DiscoveryConfig {
            provider: "static".to_string(),
            ..Default::default()
        };
        let result = create_provider(&cfg);
        assert!(result.is_ok());
    }
}

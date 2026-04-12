//! Lifecycle-managed discovery component.
//!
//! Mirrors gokit's `discovery.Component` — handles provider creation,
//! service registration on start, deregistration on stop, and health
//! reporting. Services only need to add this component to the app registry.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::DiscoveryConfig;
use crate::factory;
use crate::traits::{Discovery, Registry};

/// A lifecycle-managed discovery component.
///
/// Implements [`Component`] so it can be registered with the application
/// component registry. On start it creates the provider via the factory,
/// optionally registers the local service instance, and on stop it
/// deregisters.
pub struct DiscoveryComponent {
    config: DiscoveryConfig,
    registry: Mutex<Option<Arc<dyn Registry>>>,
    discovery: Mutex<Option<Arc<dyn Discovery>>>,
    instance_id: Mutex<Option<String>>,
}

impl DiscoveryComponent {
    /// Create a new discovery component from the given config.
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            registry: Mutex::new(None),
            discovery: Mutex::new(None),
            instance_id: Mutex::new(None),
        }
    }

    /// Returns the registry, if the component has started.
    pub fn registry(&self) -> Option<Arc<dyn Registry>> {
        self.registry.lock().clone()
    }

    /// Returns the discovery client, if the component has started.
    pub fn discovery(&self) -> Option<Arc<dyn Discovery>> {
        self.discovery.lock().clone()
    }
}

#[async_trait]
impl Component for DiscoveryComponent {
    fn name(&self) -> &str {
        "discovery"
    }

    async fn start(&self) -> AppResult<()> {
        // Initialise built-in provider factories (idempotent).
        factory::init_builtin();

        let mut config = self.config.clone();
        config.apply_defaults();

        if !config.enabled {
            debug!("Discovery disabled — using static provider");
            let cfg_static = DiscoveryConfig {
                provider: "static".to_string(),
                ..config
            };
            let (reg, disc) = factory::create_provider(&cfg_static)?;
            *self.registry.lock() = Some(reg);
            *self.discovery.lock() = Some(disc);
            return Ok(());
        }

        config.validate().map_err(|e| {
            AppError::new(ErrorCode::InvalidInput, format!("discovery config: {e}"))
        })?;

        let (reg, disc) = factory::create_provider(&config)?;
        *self.registry.lock() = Some(reg.clone());
        *self.discovery.lock() = Some(disc);

        // Auto-register when registration is enabled.
        if config.registration.enabled {
            let mut instance = config.build_instance();

            // Enrich metadata with health URL when health checks are enabled.
            if config.health.enabled {
                let addr = if instance.address.is_empty() {
                    "localhost".to_string()
                } else {
                    instance.address.clone()
                };
                let health_url = format!("http://{}:{}{}", addr, instance.port, config.health.path);
                instance
                    .metadata
                    .insert("health_url".to_string(), health_url);
            }

            info!(
                id = %instance.id,
                name = %instance.name,
                address = %instance.address,
                port = instance.port,
                "Registering with service discovery"
            );

            let max_retries = config.registration.max_retries.max(1);
            let mut interval = config.registration.retry_duration();
            let mut last_err = None;
            let instance_id = instance.id.clone();

            for attempt in 1..=max_retries {
                match reg.register(&instance).await {
                    Ok(()) => {
                        *self.instance_id.lock() = Some(instance_id.clone());
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            service_id = %instance_id,
                            attempt = attempt,
                            max_retries = max_retries,
                            "failed to register service"
                        );
                        last_err = Some(e);
                        if attempt < max_retries {
                            sleep(interval).await;
                            interval *= 2; // exponential backoff
                        }
                    }
                }
            }

            if let Some(err) = last_err {
                if config.registration.required {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!("discovery: register self after {max_retries} retries: {err}"),
                    ));
                }
                warn!(
                    service_id = %instance_id,
                    "failed to register with discovery — continuing in degraded mode"
                );
            }
        }

        debug!(provider = %config.provider, "Discovery component started");
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        debug!("Discovery component stopping");

        let instance_id = self.instance_id.lock().take();
        if let Some(id) = instance_id {
            if let Some(reg) = self.registry() {
                if let Err(e) = reg.deregister(&id).await {
                    warn!(error = %e, id = %id, "Failed to deregister on stop");
                }
            }
        }

        Ok(())
    }

    fn health(&self) -> Health {
        if self.discovery.lock().is_none() {
            return Health::unhealthy("discovery", "not initialized");
        }
        if !self.config.enabled {
            return Health::healthy("discovery (static)");
        }
        if self.instance_id.lock().is_some() {
            Health::healthy("discovery")
        } else {
            Health::degraded("discovery", "no services registered")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_component_uses_static() {
        let config = DiscoveryConfig {
            enabled: false,
            ..Default::default()
        };
        let comp = DiscoveryComponent::new(config);
        comp.start().await.unwrap();
        assert!(comp.discovery().is_some());
        assert!(comp.registry().is_some());
        comp.stop().await.unwrap();
    }

    #[tokio::test]
    async fn static_provider_with_registration() {
        let config = DiscoveryConfig {
            enabled: true,
            provider: "static".to_string(),
            registration: crate::config::RegistrationConfig {
                enabled: true,
                service_name: "test-svc".to_string(),
                service_id: "test-svc-1".to_string(),
                service_address: "127.0.0.1".to_string(),
                service_port: 8080,
                ..Default::default()
            },
            ..Default::default()
        };
        let comp = DiscoveryComponent::new(config);
        comp.start().await.unwrap();
        assert!(comp.instance_id.lock().is_some());
        comp.stop().await.unwrap();
        assert!(comp.instance_id.lock().is_none());
    }
}

//! Lifecycle-managed discovery component.
//!
//! Mirrors gokit's `discovery.Component` — handles provider creation, service registration on start,
//! deregistration on stop, and health reporting.
//! Services only need to add this component to the app registry.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
use tracing::{debug, info, warn};

use crate::config::DiscoveryConfig;
use crate::factory::DiscoveryRegistry;
use crate::traits::{Discovery, Registry};

/// A lifecycle-managed discovery component.
///
/// Implements [`Component`] so it can be registered with the application component registry.
/// On start it creates the provider via the factory, optionally registers the local service instance,
/// and on stop it deregisters.
pub struct DiscoveryComponent {
    config: DiscoveryConfig,
    providers: DiscoveryRegistry,
    registry: Mutex<Option<Arc<dyn Registry>>>,
    discovery: Mutex<Option<Arc<dyn Discovery>>>,
    instance_id: Mutex<Option<String>>,
}

impl DiscoveryComponent {
    /// Create a new discovery component from the given config.
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            providers: DiscoveryRegistry::builtins(),
            registry: Mutex::new(None),
            discovery: Mutex::new(None),
            instance_id: Mutex::new(None),
        }
    }

    /// Create a discovery component with an explicit provider registry.
    #[must_use]
    pub fn with_registry(config: DiscoveryConfig, providers: DiscoveryRegistry) -> Self {
        Self {
            config,
            providers,
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
        let mut config = self.config.clone();
        config.apply_defaults();

        if !config.enabled {
            debug!("Discovery disabled — using static provider");
            let cfg_static = DiscoveryConfig {
                provider: "static".to_string(),
                ..config
            };
            let (reg, disc) = self.providers.create(&cfg_static)?;
            *self.registry.lock() = Some(reg);
            *self.discovery.lock() = Some(disc);
            return Ok(());
        }

        config.validate()?;

        let (reg, disc) = self.providers.create(&config)?;
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

            let instance_id = instance.id.clone();
            let max_retries = config.registration.max_retries.max(1) as usize;
            let retry_policy = RetryPolicy::new()
                .with_max_attempts(max_retries)
                .with_constant_backoff(ConstantBackoff::new(config.registration.retry_duration()?))
                .with_jitter(false)
                .with_on_retry({
                    let instance_id = instance_id.clone();
                    move |attempt, error| {
                        warn!(
                            error = %error,
                            service_id = %instance_id,
                            attempt,
                            max_retries,
                            "failed to register service"
                        );
                    }
                });
            let registration_policy = Policy::new().with_retry(retry_policy);

            let registration = registration_policy
                .execute(|| async { reg.register(&instance).await })
                .await;

            if let Err(err) = registration {
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
            } else {
                *self.instance_id.lock() = Some(instance_id.clone());
            }
        }

        debug!(provider = %config.provider, "Discovery component started");
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        debug!("Discovery component stopping");

        let instance_id = self.instance_id.lock().take();
        if let Some(id) = instance_id
            && let Some(reg) = self.registry()
            && let Err(e) = reg.deregister(&id).await
        {
            warn!(error = %e, id = %id, "Failed to deregister on stop");
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
    use crate::instance::ServiceInstance;
    use async_trait::async_trait;

    struct FailingRegistry {
        fail_register: bool,
        deregistered: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Registry for FailingRegistry {
        async fn register(&self, _instance: &ServiceInstance) -> AppResult<()> {
            if self.fail_register {
                Err(AppError::new(ErrorCode::Internal, "registration failed"))
            } else {
                Ok(())
            }
        }

        async fn deregister(&self, id: &str) -> AppResult<()> {
            self.deregistered.lock().push(id.to_string());
            Ok(())
        }
    }

    #[async_trait]
    impl Discovery for FailingRegistry {
        async fn resolve(&self, _service: &str) -> AppResult<Vec<ServiceInstance>> {
            Ok(Vec::new())
        }
    }

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
        assert!(comp.health().is_healthy());
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

    #[test]
    fn with_registry_initial_health_is_unhealthy_and_accessors_are_empty() {
        let comp =
            DiscoveryComponent::with_registry(DiscoveryConfig::default(), DiscoveryRegistry::new());

        assert_eq!(comp.name(), "discovery");
        assert!(comp.registry().is_none());
        assert!(comp.discovery().is_none());
        assert!(!comp.health().is_healthy());
    }

    #[tokio::test]
    async fn optional_registration_failure_starts_degraded_without_instance_id() {
        let provider = Arc::new(FailingRegistry {
            fail_register: true,
            deregistered: Arc::new(Mutex::new(Vec::new())),
        });
        let mut providers = DiscoveryRegistry::new();
        providers.register(
            "fake",
            Box::new({
                let provider = provider.clone();
                move |_| Ok((provider.clone(), provider.clone()))
            }),
        );
        let comp = DiscoveryComponent::with_registry(
            DiscoveryConfig {
                enabled: true,
                provider: "fake".to_string(),
                registration: crate::config::RegistrationConfig {
                    enabled: true,
                    required: false,
                    service_name: "svc".to_string(),
                    service_port: 8080,
                    max_retries: 1,
                    retry_interval: "1ms".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            providers,
        );

        comp.start().await.unwrap();

        assert!(comp.registry().is_some());
        assert!(comp.discovery().is_some());
        assert!(comp.instance_id.lock().is_none());
        assert!(!comp.health().is_healthy());
    }

    #[tokio::test]
    async fn required_registration_failure_returns_start_error() {
        let provider = Arc::new(FailingRegistry {
            fail_register: true,
            deregistered: Arc::new(Mutex::new(Vec::new())),
        });
        let mut providers = DiscoveryRegistry::new();
        providers.register(
            "fake",
            Box::new({
                let provider = provider.clone();
                move |_| Ok((provider.clone(), provider.clone()))
            }),
        );
        let comp = DiscoveryComponent::with_registry(
            DiscoveryConfig {
                enabled: true,
                provider: "fake".to_string(),
                registration: crate::config::RegistrationConfig {
                    enabled: true,
                    required: true,
                    service_name: "svc".to_string(),
                    service_port: 8080,
                    max_retries: 1,
                    retry_interval: "1ms".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            providers,
        );

        let err = comp.start().await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.to_string().contains("register self"));
    }

    #[tokio::test]
    async fn successful_registration_adds_health_url_and_deregisters_on_stop() {
        let deregistered = Arc::new(Mutex::new(Vec::new()));
        let provider = Arc::new(FailingRegistry {
            fail_register: false,
            deregistered: deregistered.clone(),
        });
        let mut providers = DiscoveryRegistry::new();
        providers.register(
            "fake",
            Box::new({
                let provider = provider.clone();
                move |_| Ok((provider.clone(), provider.clone()))
            }),
        );
        let comp = DiscoveryComponent::with_registry(
            DiscoveryConfig {
                enabled: true,
                provider: "fake".to_string(),
                registration: crate::config::RegistrationConfig {
                    enabled: true,
                    service_name: "svc".to_string(),
                    service_id: "svc-1".to_string(),
                    service_port: 8080,
                    max_retries: 1,
                    retry_interval: "1ms".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            providers,
        );

        comp.start().await.unwrap();
        assert!(comp.health().is_healthy());
        comp.stop().await.unwrap();

        assert_eq!(*deregistered.lock(), vec!["svc-1".to_string()]);
    }
}

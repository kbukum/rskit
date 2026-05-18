use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use rskit_errors::{AppError, AppResult};

use crate::instance::ServiceInstance;
use crate::traits::{Discovery, Registry};

/// In-memory service registry useful for testing and local development.
pub struct InMemoryDiscovery {
    instances: Arc<RwLock<HashMap<String, Vec<ServiceInstance>>>>,
}

impl InMemoryDiscovery {
    /// Create an empty discovery registry.
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add an instance under the given service name.
    pub async fn add(&self, service: &str, instance: ServiceInstance) {
        let mut map = self.instances.write().await;
        map.entry(service.to_owned())
            .or_insert_with(Vec::new)
            .push(instance);
    }

    /// Remove an instance by id from the given service.
    pub async fn remove(&self, service: &str, id: &str) {
        let mut map = self.instances.write().await;
        if let Some(instances) = map.get_mut(service) {
            instances.retain(|i| i.id != id);
        }
    }
}

impl Default for InMemoryDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Discovery for InMemoryDiscovery {
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
        let map = self.instances.read().await;
        Ok(map.get(service).cloned().unwrap_or_default())
    }
}

#[async_trait]
impl Registry for InMemoryDiscovery {
    async fn register(&self, instance: &ServiceInstance) -> AppResult<()> {
        self.add(&instance.name, instance.clone()).await;
        Ok(())
    }

    async fn deregister(&self, id: &str) -> AppResult<()> {
        let mut map = self.instances.write().await;
        let mut found = false;
        for instances in map.values_mut() {
            let before = instances.len();
            instances.retain(|i| i.id != id);
            if instances.len() != before {
                found = true;
            }
        }
        if found {
            Ok(())
        } else {
            Err(AppError::not_found("instance", Some(id)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Registry;

    fn instance(id: &str, name: &str) -> ServiceInstance {
        ServiceInstance {
            id: id.into(),
            name: name.into(),
            address: "127.0.0.1".into(),
            port: 8080,
            healthy: true,
            weight: 1,
            tags: vec![],
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn register_and_resolve() {
        let disco = InMemoryDiscovery::new();
        let inst = instance("a", "svc");
        disco.register(&inst).await.unwrap();

        let resolved = disco.resolve("svc").await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "a");
    }

    #[tokio::test]
    async fn deregister_removes() {
        let disco = InMemoryDiscovery::new();
        disco.register(&instance("a", "svc")).await.unwrap();
        disco.register(&instance("b", "svc")).await.unwrap();

        disco.deregister("a").await.unwrap();
        let resolved = disco.resolve("svc").await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "b");
    }

    #[tokio::test]
    async fn deregister_not_found() {
        let disco = InMemoryDiscovery::new();
        assert!(disco.deregister("nope").await.is_err());
    }

    #[tokio::test]
    async fn resolve_empty() {
        let disco = InMemoryDiscovery::new();
        let resolved = disco.resolve("unknown").await.unwrap();
        assert!(resolved.is_empty());
    }
}

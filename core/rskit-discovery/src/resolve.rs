//! Bootstrap-time address resolution utilities.
//!
//! [`resolve_addr`] resolves a service name to a `(host, port)` pair using the
//! [`Discovery`] trait. This is intended for one-shot infrastructure resolution
//! at startup — before connection pools are created — not for runtime
//! load balancing.

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::traits::Discovery;

/// Resolve a service name to a `(host, port)` pair via service discovery.
///
/// Returns the first healthy instance's address and port. Use this at bootstrap
/// time to resolve infrastructure addresses (database, redis, kafka, etc.)
/// before connection pools are created.
pub async fn resolve_addr(disc: &dyn Discovery, service: &str) -> AppResult<(String, u16)> {
    let instances = disc.resolve(service).await?;

    let inst = instances
        .iter()
        .find(|instance| instance.healthy)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("resolve \"{service}\": no healthy instances found"),
            )
        })?;

    Ok((inst.address.clone(), inst.port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StubDiscovery {
        instances: Vec<crate::instance::ServiceInstance>,
    }

    #[async_trait]
    impl Discovery for StubDiscovery {
        async fn resolve(
            &self,
            _service: &str,
        ) -> AppResult<Vec<crate::instance::ServiceInstance>> {
            Ok(self.instances.clone())
        }
    }

    #[tokio::test]
    async fn resolve_addr_returns_first_healthy_instance() {
        let disc = StubDiscovery {
            instances: vec![
                instance("10.0.0.1", 8080, false),
                instance("10.0.0.2", 9090, true),
            ],
        };

        assert_eq!(
            resolve_addr(&disc, "api").await.unwrap(),
            ("10.0.0.2".to_string(), 9090)
        );
    }

    #[tokio::test]
    async fn resolve_addr_errors_when_no_healthy_instances_exist() {
        let disc = StubDiscovery {
            instances: vec![instance("10.0.0.1", 8080, false)],
        };

        let err = resolve_addr(&disc, "api").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
        assert!(err.to_string().contains("no healthy instances"));
    }

    fn instance(address: &str, port: u16, healthy: bool) -> crate::instance::ServiceInstance {
        crate::instance::ServiceInstance {
            id: format!("{address}:{port}"),
            name: "api".to_string(),
            address: address.to_string(),
            port,
            healthy,
            weight: 1,
            tags: Vec::new(),
            metadata: Default::default(),
        }
    }
}

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use rskit_errors::{AppError, AppResult};

use crate::instance::ServiceInstance;
use crate::traits::{Discovery, Registry};

// ── Consul API request/response types ────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RegisterPayload {
    #[serde(rename = "ID")]
    id: String,
    name: String,
    address: String,
    port: u16,
    meta: HashMap<String, String>,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check: Option<HealthCheck>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct HealthCheck {
    #[serde(rename = "HTTP")]
    http: String,
    interval: String,
    timeout: String,
    deregister_critical_service_after: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HealthEntry {
    service: ConsulService,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ConsulService {
    #[serde(rename = "ID")]
    id: String,
    service: String,
    address: String,
    port: u16,
    #[serde(default)]
    meta: HashMap<String, String>,
    #[serde(default)]
    tags: Vec<String>,
}

// ── ConsulDiscovery ──────────────────────────────────────────────────────────

/// Consul-backed service discovery.
///
/// Uses the Consul HTTP API v1 for service registration and health-based
/// discovery.
pub struct ConsulDiscovery {
    base_url: String,
    client: Client,
    token: Option<String>,
}

impl ConsulDiscovery {
    /// Create a new Consul discovery client.
    ///
    /// `address` should be in the form `"host:port"` (e.g. `"localhost:8500"`).
    /// An optional ACL token is sent as `X-Consul-Token` on every request.
    pub fn new(address: &str, token: Option<String>) -> Self {
        Self {
            base_url: format!("http://{address}"),
            client: Client::new(),
            token,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);
        if let Some(tok) = &self.token {
            req = req.header("X-Consul-Token", tok);
        }
        req
    }
}

#[async_trait]
impl Discovery for ConsulDiscovery {
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
        debug!(service, "resolving service instances from consul");

        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/v1/health/service/{service}?passing=true"),
            )
            .send()
            .await
            .map_err(|e| AppError::external_service("consul", e))?;

        if !resp.status().is_success() {
            warn!(
                service,
                status = %resp.status(),
                "consul resolve returned non-success status"
            );
            return Err(AppError::external_service(
                "consul",
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("consul returned status {}", resp.status()),
                ),
            ));
        }

        let entries: Vec<HealthEntry> = resp
            .json()
            .await
            .map_err(|e| AppError::external_service("consul", e))?;

        let instances = entries
            .into_iter()
            .map(|entry| {
                let svc = entry.service;
                ServiceInstance {
                    id: svc.id,
                    name: svc.service,
                    address: svc.address,
                    port: svc.port,
                    healthy: true, // only passing services are returned
                    tags: svc.tags,
                    metadata: svc.meta,
                }
            })
            .collect();

        Ok(instances)
    }
}

#[async_trait]
impl Registry for ConsulDiscovery {
    async fn register(&self, instance: &ServiceInstance) -> AppResult<()> {
        debug!(id = %instance.id, name = %instance.name, "registering instance with consul");

        let check = instance.metadata.get("health_url").map(|url| HealthCheck {
            http: url.clone(),
            interval: "10s".to_owned(),
            timeout: "5s".to_owned(),
            deregister_critical_service_after: "1m".to_owned(),
        });

        let payload = RegisterPayload {
            id: instance.id.clone(),
            name: instance.name.clone(),
            address: instance.address.clone(),
            port: instance.port,
            meta: instance.metadata.clone(),
            tags: instance.tags.clone(),
            check,
        };

        let resp = self
            .request(reqwest::Method::PUT, "/v1/agent/service/register")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::external_service("consul", e))?;

        if !resp.status().is_success() {
            warn!(
                id = %instance.id,
                status = %resp.status(),
                "consul register returned non-success status"
            );
            return Err(AppError::external_service(
                "consul",
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("consul returned status {}", resp.status()),
                ),
            ));
        }

        Ok(())
    }

    async fn deregister(&self, id: &str) -> AppResult<()> {
        debug!(id, "deregistering instance from consul");

        let resp = self
            .request(
                reqwest::Method::PUT,
                &format!("/v1/agent/service/deregister/{id}"),
            )
            .send()
            .await
            .map_err(|e| AppError::external_service("consul", e))?;

        if !resp.status().is_success() {
            warn!(
                id,
                status = %resp.status(),
                "consul deregister returned non-success status"
            );
            return Err(AppError::external_service(
                "consul",
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("consul returned status {}", resp.status()),
                ),
            ));
        }

        Ok(())
    }
}

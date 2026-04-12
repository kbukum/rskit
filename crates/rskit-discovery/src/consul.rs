use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use rskit_errors::{AppError, AppResult};

use crate::instance::ServiceInstance;
use crate::traits::{Discovery, Registry, Watcher};

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

/// Default polling interval when using the Watcher fallback.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Consul blocking-query based watcher.
///
/// Uses the `?index=` long-poll parameter on the health endpoint.  When the
/// Consul index doesn't advance within a cycle the response is treated as
/// "no change" and no message is emitted.  If blocking queries are not
/// supported (e.g. mock server), the implementation falls back to simple
/// polling with change-detection.
#[async_trait]
impl Watcher for ConsulDiscovery {
    async fn watch(
        &self,
        service: &str,
    ) -> AppResult<mpsc::Receiver<Vec<ServiceInstance>>> {
        let (tx, rx) = mpsc::channel(16);
        let base_url = self.base_url.clone();
        let token = self.token.clone();
        let client = self.client.clone();
        let service = service.to_owned();

        tokio::spawn(async move {
            let mut last_index: u64 = 0;
            let mut last_endpoints: Vec<String> = Vec::new();

            loop {
                let url = format!(
                    "{}/v1/health/service/{}?passing=true&index={}&wait=30s",
                    base_url, service, last_index,
                );
                let mut req = client.request(reqwest::Method::GET, &url);
                if let Some(tok) = &token {
                    req = req.header("X-Consul-Token", tok);
                }

                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        // Extract the X-Consul-Index header for long-polling
                        let new_index = resp
                            .headers()
                            .get("X-Consul-Index")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(0);

                        match resp.json::<Vec<HealthEntry>>().await {
                            Ok(entries) => {
                                let instances: Vec<ServiceInstance> = entries
                                    .into_iter()
                                    .map(|entry| {
                                        let svc = entry.service;
                                        ServiceInstance {
                                            id: svc.id,
                                            name: svc.service,
                                            address: svc.address,
                                            port: svc.port,
                                            healthy: true,
                                            tags: svc.tags,
                                            metadata: svc.meta,
                                        }
                                    })
                                    .collect();

                                // Emit only when the set actually changed
                                let endpoints: Vec<String> =
                                    instances.iter().map(|i| i.endpoint()).collect();

                                if endpoints != last_endpoints || new_index != last_index {
                                    last_endpoints = endpoints;
                                    if tx.send(instances).await.is_err() {
                                        debug!(
                                            service = %service,
                                            "watch receiver dropped, stopping"
                                        );
                                        return;
                                    }
                                }

                                last_index = new_index;
                            }
                            Err(e) => {
                                warn!(
                                    service = %service,
                                    error = %e,
                                    "consul watch: failed to parse response"
                                );
                                tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
                            }
                        }
                    }
                    Ok(resp) => {
                        warn!(
                            service = %service,
                            status = %resp.status(),
                            "consul watch: non-success status"
                        );
                        tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
                    }
                    Err(e) => {
                        warn!(
                            service = %service,
                            error = %e,
                            "consul watch: request failed"
                        );
                        tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
                    }
                }
            }
        });

        Ok(rx)
    }
}

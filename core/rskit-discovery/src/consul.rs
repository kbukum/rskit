use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use rskit_httpclient::{ErrorResponse, HttpClient, HttpClientConfig, Request};
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
    client: HttpClient,
}

const CONSUL_BLOCKING_WAIT_SECS: u64 = 30;
const CONSUL_REQUEST_TIMEOUT_GRACE_SECS: u64 = 5;
const CONSUL_BLOCKING_WAIT: Duration = Duration::from_secs(CONSUL_BLOCKING_WAIT_SECS);
const CONSUL_REQUEST_TIMEOUT: Duration =
    Duration::from_secs(CONSUL_BLOCKING_WAIT_SECS + CONSUL_REQUEST_TIMEOUT_GRACE_SECS);

impl ConsulDiscovery {
    /// Create a new Consul discovery client.
    ///
    /// `address` should be in the form `"host:port"` (e.g. `"localhost:8500"`).
    /// An optional ACL token is sent as `X-Consul-Token` on every request.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be constructed.
    pub fn new(address: &str, token: Option<String>) -> AppResult<Self> {
        let mut config = consul_http_config(address);
        if let Some(token) = token {
            config = config.with_header("X-Consul-Token", token);
        }

        Ok(Self {
            client: HttpClient::new(config)?,
        })
    }
}

fn consul_http_config(address: &str) -> HttpClientConfig {
    HttpClientConfig::new()
        .with_base_url(format!("http://{address}"))
        .with_timeout(CONSUL_REQUEST_TIMEOUT)
}

#[async_trait]
impl Discovery for ConsulDiscovery {
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>> {
        debug!(service, "resolving service instances from consul");

        let resp = self
            .client
            .send(
                Request::get(format!("/v1/health/service/{service}"))
                    .query_param("passing", "true"),
            )
            .await
            .map_err(|error| consul_context(error, "consul resolve request failed"))?
            .error_for_status_with(consul_status_error)?;
        let entries = resp
            .json()
            .map_err(|error| consul_context(error, "failed to parse consul response"))?;

        Ok(entries_to_instances(entries))
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

        self.client
            .send(
                Request::put("/v1/agent/service/register")
                    .json_body(&payload)
                    .map_err(|e| {
                        AppError::new(
                            rskit_errors::ErrorCode::InvalidInput,
                            format!("failed to serialize consul register payload: {e}"),
                        )
                    })?,
            )
            .await
            .map_err(|error| consul_context(error, "consul register request failed"))?
            .error_for_status_with(consul_status_error)?;

        Ok(())
    }

    async fn deregister(&self, id: &str) -> AppResult<()> {
        debug!(id, "deregistering instance from consul");

        self.client
            .send(Request::put(format!("/v1/agent/service/deregister/{id}")))
            .await
            .map_err(|error| consul_context(error, "consul deregister request failed"))?
            .error_for_status_with(consul_status_error)?;

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
    async fn watch(&self, service: &str) -> AppResult<mpsc::Receiver<Vec<ServiceInstance>>> {
        let (tx, rx) = mpsc::channel(16);
        let client = self.client.clone();
        let service = service.to_owned();

        tokio::spawn(async move {
            let mut last_index: u64 = 0;
            let mut last_endpoints: Vec<String> = Vec::new();

            loop {
                let request = Request::get(format!("/v1/health/service/{service}"))
                    .query_param("passing", "true")
                    .query_param("index", last_index.to_string())
                    .query_param("wait", consul_blocking_wait_query());

                match client.send(request).await {
                    Ok(resp) if resp.is_success() => {
                        // Extract the X-Consul-Index header for long-polling
                        let new_index = resp
                            .header("X-Consul-Index")
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(0);

                        match resp.json::<Vec<HealthEntry>>() {
                            Ok(entries) => {
                                let instances = entries_to_instances(entries);

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

fn consul_blocking_wait_query() -> String {
    format!("{}s", CONSUL_BLOCKING_WAIT.as_secs())
}

fn consul_context(error: AppError, context: &'static str) -> AppError {
    error.context(context).with_detail("service", "consul")
}

fn entries_to_instances(entries: Vec<HealthEntry>) -> Vec<ServiceInstance> {
    entries
        .into_iter()
        .map(|entry| {
            let svc = entry.service;
            ServiceInstance {
                id: svc.id,
                name: svc.service,
                address: svc.address,
                port: svc.port,
                healthy: true,
                weight: 1,
                tags: svc.tags,
                metadata: svc.meta,
            }
        })
        .collect()
}

fn consul_status_error(response: ErrorResponse) -> AppError {
    warn!(
        status = %response.status,
        "consul returned non-success status"
    );

    AppError::new(
        rskit_errors::ErrorCode::ExternalService,
        format!("consul returned status {}", response.status),
    )
    .with_detail("status", response.status.as_u16().to_string())
    .with_detail("body", response.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consul_http_timeout_exceeds_blocking_watch_wait() {
        let config = consul_http_config("localhost:8500");

        assert!(config.timeout > CONSUL_BLOCKING_WAIT);
        assert_eq!(config.timeout, CONSUL_REQUEST_TIMEOUT);
        assert_eq!(consul_blocking_wait_query(), "30s");
    }

    #[test]
    fn consul_constructor_applies_token_and_timeout() {
        let consul = ConsulDiscovery::new("localhost:8500", Some("secret".to_owned()))
            .expect("consul discovery client should build");
        let config = consul.client.config();

        assert_eq!(
            config.default_headers.get("X-Consul-Token"),
            Some(&"secret".to_owned())
        );
        assert!(config.timeout > CONSUL_BLOCKING_WAIT);
    }
}

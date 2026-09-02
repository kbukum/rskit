use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use rskit_httpclient::{DestinationPolicy, ErrorResponse, HttpClient, HttpClientConfig, Request};
use rskit_stream::{SpawnedTask, TaskGroup};
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
/// Uses the Consul HTTP API v1 for service registration and health-based discovery.
pub struct ConsulDiscovery {
    client: HttpClient,
    /// Owns spawned watch tasks so they are cancelled and drained on drop.
    watch_tasks: Arc<Mutex<TaskGroup>>,
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
            watch_tasks: Arc::new(Mutex::new(TaskGroup::new())),
        })
    }
}

impl Drop for ConsulDiscovery {
    fn drop(&mut self) {
        // Stop long-poll watch tasks promptly rather than letting them block on Consul until the next observed change.
        self.watch_tasks.lock().cancel_all();
    }
}

fn consul_http_config(address: &str) -> HttpClientConfig {
    let base_url = format!("http://{address}");
    let allowed_host = consul_address_host(address);
    HttpClientConfig::new()
        .with_base_url(base_url)
        .with_timeout(CONSUL_REQUEST_TIMEOUT)
        .with_follow_redirects(false)
        .with_destination_policy(DestinationPolicy::new().with_allowed_hosts([allowed_host]))
}

fn consul_address_host(address: &str) -> String {
    if let Some(rest) = address.strip_prefix('[')
        && let Some((host, _)) = rest.split_once(']')
    {
        return host.to_owned();
    }
    if let Ok(addr) = address.parse::<std::net::SocketAddr>() {
        return addr.ip().to_string();
    }
    address
        .split_once(':')
        .map_or(address, |(host, _)| host)
        .to_owned()
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
/// Uses the `?index=` long-poll parameter on the health endpoint.
/// When the Consul index doesn't advance within a cycle the response is treated as "no change"
/// and no message is emitted. If blocking queries are not supported (e.g. mock server),
/// the implementation falls back to simple polling with change-detection.
#[async_trait]
impl Watcher for ConsulDiscovery {
    async fn watch(&self, service: &str) -> AppResult<mpsc::Receiver<Vec<ServiceInstance>>> {
        let (tx, rx) = mpsc::channel(16);
        let client = self.client.clone();
        let service = service.to_owned();

        let task = SpawnedTask::spawn(move |cancel| async move {
            let mut last_index: u64 = 0;
            let mut last_endpoints: Vec<String> = Vec::new();

            loop {
                let request = Request::get(format!("/v1/health/service/{service}"))
                    .query_param("passing", "true")
                    .query_param("index", last_index.to_string())
                    .query_param("wait", consul_blocking_wait_query());

                let resp = tokio::select! {
                    () = cancel.cancelled() => {
                        debug!(service = %service, "watch cancelled, stopping");
                        return;
                    }
                    result = client.send(request) => result,
                };

                match resp {
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

        self.watch_tasks.lock().push(task);
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
    let last_seen = rskit_util::time::now_rfc3339();
    entries
        .into_iter()
        .map(|entry| {
            let svc = entry.service;
            let protocol = instance_protocol(&svc.meta, &svc.tags);
            let weight = svc
                .meta
                .get("weight")
                .and_then(|w| w.parse::<u32>().ok())
                .unwrap_or(1);
            ServiceInstance {
                id: svc.id,
                name: svc.service,
                address: svc.address,
                port: svc.port,
                protocol,
                // Discovery queries Consul with `passing=true`, so every returned entry is healthy.
                health: crate::instance::HealthState::Healthy,
                weight,
                tags: svc.tags,
                metadata: svc.meta,
                last_seen: last_seen.clone(),
            }
        })
        .collect()
}

/// Derive an instance protocol from Consul service metadata, falling back to a well-known
/// protocol tag and finally to an empty string when neither is present.
fn instance_protocol(meta: &HashMap<String, String>, tags: &[String]) -> String {
    if let Some(protocol) = meta.get("protocol") {
        return protocol.clone();
    }
    tags.iter()
        .find(|tag| matches!(tag.as_str(), "http" | "grpc" | "websocket"))
        .cloned()
        .unwrap_or_default()
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
    use rskit_testutil::{FakeHttpServer, FakeResponse};

    #[test]
    fn consul_http_timeout_exceeds_blocking_watch_wait() {
        let config = consul_http_config("localhost:8500");

        assert!(config.timeout > CONSUL_BLOCKING_WAIT);
        assert_eq!(config.timeout, CONSUL_REQUEST_TIMEOUT);
        assert_eq!(consul_blocking_wait_query(), "30s");
        assert!(!config.follow_redirects);
        assert_eq!(
            config.destination_policy.allowed_hosts,
            vec!["localhost".to_owned()]
        );
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

    #[test]
    fn consul_allowed_host_handles_ipv6_socket_addresses() {
        let config = consul_http_config("[::1]:8500");

        assert_eq!(
            config.destination_policy.allowed_hosts,
            vec!["::1".to_owned()]
        );
    }

    #[test]
    fn consul_allowed_host_handles_hostnames_and_ipv4_socket_addresses() {
        assert_eq!(
            consul_address_host("consul.service.local:8500"),
            "consul.service.local"
        );
        assert_eq!(consul_address_host("127.0.0.1:8500"), "127.0.0.1");
        assert_eq!(consul_address_host("localhost"), "localhost");
    }

    #[tokio::test]
    async fn consul_resolve_maps_health_entries_from_local_http_server() {
        let body = r#"[{"Service":{"ID":"users-1","Service":"users","Address":"127.0.0.1","Port":8080,"Meta":{"zone":"a"},"Tags":["blue"]}}]"#;
        let server = FakeHttpServer::serve_once(
            FakeResponse::new(200)
                .with_header("X-Consul-Index", "7")
                .with_body(body),
        )
        .await
        .unwrap();
        let consul = ConsulDiscovery::new(&server.address(), None).unwrap();

        let instances = consul.resolve("users").await.unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "users-1");
        assert_eq!(instances[0].endpoint(), "127.0.0.1:8080");
        assert_eq!(instances[0].tags, vec!["blue"]);
        assert_eq!(
            instances[0].metadata.get("zone").map(String::as_str),
            Some("a")
        );
        let request = server.captured_request().await.unwrap();
        assert!(
            request
                .request_line()
                .starts_with("GET /v1/health/service/users?passing=true ")
        );
    }

    #[test]
    fn entries_to_instances_populates_protocol_weight_and_last_seen() {
        let body = r#"[{"Service":{"ID":"api-1","Service":"api","Address":"10.0.0.5","Port":9090,"Meta":{"protocol":"grpc","weight":"7"},"Tags":["canary"]}}]"#;
        let entries: Vec<HealthEntry> = serde_json::from_str(body).unwrap();

        let instances = entries_to_instances(entries);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].protocol, "grpc");
        assert_eq!(instances[0].weight, 7);
        assert!(instances[0].is_healthy());
        assert!(instances[0].last_seen.is_some());
    }

    #[test]
    fn entries_to_instances_derives_protocol_from_tag_and_defaults_weight() {
        let body = r#"[{"Service":{"ID":"web-1","Service":"web","Address":"10.0.0.6","Port":80,"Meta":{},"Tags":["primary","http"]}}]"#;
        let entries: Vec<HealthEntry> = serde_json::from_str(body).unwrap();

        let instances = entries_to_instances(entries);

        assert_eq!(instances[0].protocol, "http");
        assert_eq!(instances[0].weight, 1);
    }

    #[tokio::test]
    async fn consul_register_sends_payload_and_token_to_local_http_server() {
        let server = FakeHttpServer::serve_once(FakeResponse::new(200))
            .await
            .unwrap();
        let consul = ConsulDiscovery::new(&server.address(), Some("secret".to_owned())).unwrap();
        let mut metadata = HashMap::new();
        metadata.insert(
            "health_url".to_string(),
            "http://127.0.0.1:8080/health".to_string(),
        );
        let instance = ServiceInstance {
            id: "api-1".to_string(),
            name: "api".to_string(),
            address: "127.0.0.1".to_string(),
            port: 8080,
            protocol: String::new(),
            health: crate::instance::HealthState::Healthy,
            weight: 1,
            tags: vec!["blue".to_string()],
            metadata,
            last_seen: None,
        };

        consul.register(&instance).await.unwrap();

        let request = server.captured_request().await.unwrap();
        assert!(
            request
                .request_line()
                .starts_with("PUT /v1/agent/service/register ")
        );
        assert_eq!(request.header("x-consul-token").as_deref(), Some("secret"));
        assert!(request.contains(r#""ID":"api-1""#));
        assert!(request.contains(r#""HTTP":"http://127.0.0.1:8080/health""#));
    }

    #[tokio::test]
    async fn consul_deregister_maps_non_success_status() {
        let server = FakeHttpServer::serve_once(FakeResponse::new(503).with_body("maintenance"))
            .await
            .unwrap();
        let consul = ConsulDiscovery::new(&server.address(), None).unwrap();

        let err = consul.deregister("api-1").await.unwrap_err();

        assert_eq!(err.code(), rskit_errors::ErrorCode::ExternalService);
        assert!(err.to_string().contains("status 503"));
        let request = server.captured_request().await.unwrap();
        assert!(
            request
                .request_line()
                .starts_with("PUT /v1/agent/service/deregister/api-1 ")
        );
    }
}

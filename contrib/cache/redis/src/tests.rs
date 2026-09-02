use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_cache::{CacheConfig, CacheRegistry, CacheStore, CacheStoreFactory};
use rskit_errors::ErrorCode;
use rskit_util::SecretString;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::*;

struct FakeRedis {
    port: u16,
    task: JoinHandle<()>,
}

impl FakeRedis {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let store = Arc::new(tokio::sync::Mutex::new(HashMap::<String, String>::new()));
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(handle_fake_redis(stream, Arc::clone(&store)));
            }
        });
        Self { port, task }
    }
}

impl Drop for FakeRedis {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_fake_redis(
    stream: TcpStream,
    store: Arc<tokio::sync::Mutex<HashMap<String, String>>>,
) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    while let Some(command) = read_resp_array(&mut reader).await {
        let response = fake_redis_response(command, &store).await;
        write.write_all(response.as_bytes()).await.unwrap();
    }
}

async fn read_resp_array(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Option<Vec<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.unwrap() == 0 {
        return None;
    }
    let count = line.trim_end().strip_prefix('*')?.parse::<usize>().ok()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        values.push(line.trim_end().to_string());
    }
    Some(values)
}

async fn fake_redis_response(
    command: Vec<String>,
    store: &tokio::sync::Mutex<HashMap<String, String>>,
) -> String {
    match command.first().map(String::as_str) {
        Some("CLIENT" | "PING") => "+OK\r\n".to_string(),
        Some("GET") => match store.lock().await.get(&command[1]) {
            Some(value) => format!("${}\r\n{}\r\n", value.len(), value),
            None => "$-1\r\n".to_string(),
        },
        Some("SET") | Some("PSETEX") => {
            let value_index = if command[0] == "PSETEX" { 3 } else { 2 };
            store
                .lock()
                .await
                .insert(command[1].clone(), command[value_index].clone());
            "+OK\r\n".to_string()
        }
        Some("DEL") => format!(
            ":{}\r\n",
            usize::from(store.lock().await.remove(&command[1]).is_some())
        ),
        Some("EXISTS") => format!(
            ":{}\r\n",
            usize::from(store.lock().await.contains_key(&command[1]))
        ),
        _ => "-ERR unsupported command\r\n".to_string(),
    }
}

#[test]
fn default_config_targets_local_redis_without_prefix() {
    let config = Config::default();

    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 6379);
    assert!(config.password.is_none());
    assert_eq!(config.database, 0);
    assert_eq!(config.connect_timeout, Duration::from_secs(5));
    assert_eq!(config.operation_timeout, Duration::from_secs(5));
    assert!(config.key_prefix.is_none());
}

#[test]
fn config_round_trips_duration_as_seconds() {
    let config = Config {
        connect_timeout: Duration::from_secs(9),
        key_prefix: Some("svc".to_string()),
        ..Config::default()
    };

    let json = serde_json::to_string(&config).unwrap();
    let decoded: Config = serde_json::from_str(&json).unwrap();

    assert!(json.contains("\"connect_timeout\":9"));
    assert_eq!(decoded.connect_timeout, Duration::from_secs(9));
    assert_eq!(decoded.key_prefix.as_deref(), Some("svc"));
}

#[test]
fn config_serializes_canonical_addr_and_db_wire_shape() {
    let config = Config {
        host: "redis.example.test".to_owned(),
        port: 6380,
        database: 3,
        key_prefix: Some("svc".to_owned()),
        ..Config::default()
    };

    let json = serde_json::to_value(&config).unwrap();
    assert_eq!(json["addr"], "redis.example.test:6380");
    assert_eq!(json["db"], 3);
    assert!(json.get("host").is_none());
    assert!(json.get("port").is_none());
    assert!(json.get("database").is_none());

    // The canonical wire shape deserializes back into the same typed config.
    let decoded: Config = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.host, "redis.example.test");
    assert_eq!(decoded.port, 6380);
    assert_eq!(decoded.database, 3);
    assert_eq!(decoded.key_prefix.as_deref(), Some("svc"));
}

#[test]
fn config_accepts_shared_addr_and_db_keys() {
    let json = r#"{"addr":"redis.example.test:6380","db":3}"#;
    let config: Config = serde_json::from_str(json).unwrap();

    assert_eq!(config.host, "redis.example.test");
    assert_eq!(config.port, 6380);
    assert_eq!(config.database, 3);
}

#[test]
fn config_rejects_addr_combined_with_host_or_port() {
    let json = r#"{"addr":"redis.example.test:6380","host":"other"}"#;
    let err = serde_json::from_str::<Config>(json).unwrap_err();
    assert!(err.to_string().contains("not both"));
}

#[test]
fn config_rejects_malformed_addr() {
    assert!(serde_json::from_str::<Config>(r#"{"addr":"no-port"}"#).is_err());
    assert!(serde_json::from_str::<Config>(r#"{"addr":"host:notaport"}"#).is_err());
}

#[test]
fn connection_url_uses_auth_when_password_is_set() {
    let config = Config {
        host: "redis.example.test".to_owned(),
        port: 6380,
        password: Some(SecretString::new("secret")),
        database: 2,
        ..Config::default()
    };

    assert_eq!(
        config.connection_url(),
        "redis://:secret@redis.example.test:6380/2"
    );
}

#[test]
fn connection_url_omits_auth_without_password() {
    let config = Config {
        host: "redis.example.test".to_owned(),
        port: 6380,
        database: 2,
        ..Config::default()
    };

    assert_eq!(config.connection_url(), "redis://redis.example.test:6380/2");
}

#[test]
fn debug_masks_password() {
    let config = Config {
        password: Some(SecretString::new("secret")),
        ..Config::default()
    };

    let debug = format!("{config:?}");

    assert!(!debug.contains("secret"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn debug_omits_redaction_marker_when_password_is_absent() {
    let config = Config::default();

    let debug = format!("{config:?}");

    assert!(debug.contains("password: None"));
    assert!(!debug.contains("<redacted>"));
}

#[test]
fn ttl_millis_rounds_sub_millisecond_up() {
    assert_eq!(redis_ttl_millis(Duration::from_nanos(1)).unwrap(), 1);
}

#[test]
fn ttl_millis_rejects_overflow() {
    let err = redis_ttl_millis(Duration::from_secs(u64::MAX))
        .expect_err("huge TTL should overflow Redis milliseconds");

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[test]
fn redis_errors_preserve_external_service_context() {
    let redis_error = redis::RedisError::from((
        redis::ErrorKind::UnexpectedReturnType,
        "bad redis response",
        "expected string".to_string(),
    ));

    let app_error = redis_err(redis_error);

    assert_eq!(app_error.code(), ErrorCode::ExternalService);
    assert!(app_error.message().contains("redis error:"));
    assert!(app_error.cause().is_some());
}

#[test]
fn key_prefix_is_applied_when_configured() {
    assert_eq!(prefixed_key("user:1", Some("app")), "app:user:1");
    assert_eq!(prefixed_key("user:1", None), "user:1");
}

#[test]
fn key_prefix_keeps_empty_prefix_explicit() {
    assert_eq!(prefixed_key("user:1", Some("")), ":user:1");
}

#[tokio::test]
async fn zero_connect_timeout_is_rejected_without_network() {
    let config = Config {
        connect_timeout: Duration::ZERO,
        ..Config::default()
    };

    let err = RedisClient::new(config).await.err().unwrap();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected_without_network() {
    let config = Config {
        operation_timeout: Duration::ZERO,
        ..Config::default()
    };

    let err = RedisClient::new(config).await.err().unwrap();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn invalid_connection_url_is_rejected_before_connecting() {
    let config = Config {
        host: "bad host".to_string(),
        ..Config::default()
    };

    let err = RedisClient::new(config).await.err().unwrap();

    assert_eq!(err.code(), ErrorCode::ConnectionFailed);
    assert!(err.cause().is_some());
}

#[tokio::test]
async fn connection_failures_are_reported_with_context() {
    let config = Config {
        host: "127.0.0.1".to_string(),
        port: 1,
        connect_timeout: Duration::from_millis(50),
        ..Config::default()
    };

    let err = match RedisClient::new(config).await {
        Ok(_) => panic!("closed Redis port should fail to connect"),
        Err(err) => err,
    };

    assert_eq!(err.code(), ErrorCode::ConnectionFailed);
}

#[tokio::test]
async fn cache_operations_use_prefixed_keys_against_redis_protocol() {
    let server = FakeRedis::start().await;
    let client = RedisClient::new(Config {
        port: server.port,
        key_prefix: Some("svc".to_string()),
        ..Config::default()
    })
    .await
    .unwrap();

    assert_eq!(client.get("missing").await.unwrap(), None);
    client.set("plain", b"value", None).await.unwrap();
    assert_eq!(
        client.get("plain").await.unwrap().as_deref(),
        Some(b"value".as_slice())
    );
    assert!(client.exists("plain").await.unwrap());
    assert!(client.delete("plain").await.unwrap());
    assert!(!client.exists("plain").await.unwrap());
    assert!(!client.delete("plain").await.unwrap());
}

#[tokio::test]
async fn cache_set_accepts_positive_ttl_and_rejects_zero_ttl() {
    let server = FakeRedis::start().await;
    let client = RedisClient::new(Config {
        port: server.port,
        ..Config::default()
    })
    .await
    .unwrap();

    client
        .set("ttl", b"value", Some(Duration::from_millis(1)))
        .await
        .unwrap();
    let err = client
        .set("ttl", b"value", Some(Duration::ZERO))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn factory_uses_cache_prefix_when_adapter_prefix_is_absent() {
    let factory = RedisFactory {
        config: Config {
            connect_timeout: Duration::ZERO,
            key_prefix: None,
            ..Config::default()
        },
    };
    let cache_config = CacheConfig {
        key_prefix: Some("shared".to_string()),
        ..CacheConfig::default()
    };

    let err = match factory.create(&cache_config).await {
        Ok(_) => panic!("zero connect timeout must be rejected before connecting"),
        Err(err) => err,
    };

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

/// A fake Redis that completes the connection handshake but never answers data commands,
/// used to prove the per-operation timeout fires instead of hanging forever.
struct StallingRedis {
    port: u16,
    task: JoinHandle<()>,
}

impl StallingRedis {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let (read, mut write) = stream.into_split();
                    let mut reader = BufReader::new(read);
                    while let Some(command) = read_resp_array(&mut reader).await {
                        match command.first().map(String::as_str) {
                            // Answer the handshake so `ConnectionManager::new` succeeds.
                            Some("CLIENT" | "PING" | "HELLO") => {
                                let _ = write.write_all(b"+OK\r\n").await;
                            }
                            // Every real command stalls: never write a reply.
                            _ => std::future::pending::<()>().await,
                        }
                    }
                });
            }
        });
        Self { port, task }
    }
}

impl Drop for StallingRedis {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn operation_timeout_fires_when_server_never_replies() {
    let server = StallingRedis::start().await;
    let client = RedisClient::new(Config {
        port: server.port,
        operation_timeout: Duration::from_millis(50),
        ..Config::default()
    })
    .await
    .unwrap();

    let err = client.get("key").await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::Timeout);
    assert!(err.message().contains("redis get timed out"));
}

#[test]
fn register_adds_redis_factory() {
    let mut registry = CacheRegistry::new();

    register(&mut registry, Config::default()).unwrap();

    assert!(registry.contains("redis"));
    assert_eq!(registry.len(), 1);
}

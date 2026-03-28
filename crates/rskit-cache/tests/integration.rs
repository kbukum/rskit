use std::time::Duration;

use rskit_cache::RedisConfig;

#[test]
fn default_config_has_expected_values() {
    let cfg = RedisConfig::default();
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 6379);
    assert!(cfg.password.is_none());
    assert_eq!(cfg.database, 0);
    assert_eq!(cfg.pool_size, 10);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
    assert!(cfg.key_prefix.is_none());
}

#[test]
fn connection_url_without_password() {
    let cfg = RedisConfig::default();
    assert_eq!(cfg.connection_url(), "redis://127.0.0.1:6379/0");
}

#[test]
fn connection_url_with_password() {
    let cfg = RedisConfig {
        password: Some("secret".into()),
        ..Default::default()
    };
    assert_eq!(cfg.connection_url(), "redis://:secret@127.0.0.1:6379/0");
}

#[test]
fn connection_url_custom_fields() {
    let cfg = RedisConfig {
        host: "redis.example.com".into(),
        port: 6380,
        database: 3,
        ..Default::default()
    };
    assert_eq!(cfg.connection_url(), "redis://redis.example.com:6380/3");
}

#[test]
fn config_with_key_prefix() {
    let cfg = RedisConfig {
        key_prefix: Some("myapp".into()),
        ..Default::default()
    };
    assert_eq!(cfg.key_prefix.as_deref(), Some("myapp"));
}

#[test]
fn deserialise_config_from_json() {
    let json = r#"{"host":"localhost","port":6380,"database":2,"connect_timeout":10}"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.host, "localhost");
    assert_eq!(cfg.port, 6380);
    assert_eq!(cfg.database, 2);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
    assert!(cfg.password.is_none());
}

#[test]
fn deserialise_config_uses_defaults_for_missing_fields() {
    let json = r#"{"host":"127.0.0.1"}"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.port, 6379);
    assert_eq!(cfg.pool_size, 10);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
}

#[tokio::test]
#[ignore = "requires running Redis server"]
async fn client_set_and_get() {
    let cfg = RedisConfig::default();
    let client = rskit_cache::RedisClient::new(cfg).await.unwrap();
    client.set("test_key", "hello", None).await.unwrap();
    let val = client.get("test_key").await.unwrap();
    assert_eq!(val.as_deref(), Some("hello"));
    client.delete("test_key").await.unwrap();
}

#[tokio::test]
#[ignore = "requires running Redis server"]
async fn client_delete_returns_true_when_key_exists() {
    let cfg = RedisConfig::default();
    let client = rskit_cache::RedisClient::new(cfg).await.unwrap();
    client.set("del_key", "value", None).await.unwrap();
    let removed = client.delete("del_key").await.unwrap();
    assert!(removed);
    let removed_again = client.delete("del_key").await.unwrap();
    assert!(!removed_again);
}

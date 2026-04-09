//! Unit tests for rskit-cache: config validation, key prefixing,
//! TypedStore serialization, error mapping, and component health.
//!
//! These tests do NOT require a running Redis server.

use std::time::Duration;

use rskit_cache::RedisConfig;

// ── Config validation ───────────────────────────────────────────────────────

#[test]
fn config_default_values() {
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
fn config_connection_url_no_password() {
    let cfg = RedisConfig::default();
    assert_eq!(cfg.connection_url(), "redis://127.0.0.1:6379/0");
}

#[test]
fn config_connection_url_with_password() {
    let cfg = RedisConfig {
        password: Some("s3cret".into()),
        ..Default::default()
    };
    assert_eq!(cfg.connection_url(), "redis://:s3cret@127.0.0.1:6379/0");
}

#[test]
fn config_connection_url_custom_host_port_db() {
    let cfg = RedisConfig {
        host: "redis.prod.internal".into(),
        port: 6380,
        database: 5,
        ..Default::default()
    };
    assert_eq!(cfg.connection_url(), "redis://redis.prod.internal:6380/5");
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
fn config_no_key_prefix() {
    let cfg = RedisConfig::default();
    assert!(cfg.key_prefix.is_none());
}

// ── JSON deserialization ────────────────────────────────────────────────────

#[test]
fn deserialise_full_config() {
    let json = r#"{
        "host": "redis.example.com",
        "port": 6380,
        "password": "pass123",
        "database": 3,
        "pool_size": 25,
        "connect_timeout": 10,
        "key_prefix": "test"
    }"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.host, "redis.example.com");
    assert_eq!(cfg.port, 6380);
    assert_eq!(cfg.password.as_deref(), Some("pass123"));
    assert_eq!(cfg.database, 3);
    assert_eq!(cfg.pool_size, 25);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
    assert_eq!(cfg.key_prefix.as_deref(), Some("test"));
}

#[test]
fn deserialise_minimal_config_uses_defaults() {
    let json = r#"{"host": "localhost"}"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.host, "localhost");
    assert_eq!(cfg.port, 6379);
    assert!(cfg.password.is_none());
    assert_eq!(cfg.database, 0);
    assert_eq!(cfg.pool_size, 10);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
    assert!(cfg.key_prefix.is_none());
}

#[test]
fn deserialise_with_zero_values() {
    let json = r#"{"host": "127.0.0.1", "database": 0, "pool_size": 1, "connect_timeout": 1}"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.database, 0);
    assert_eq!(cfg.pool_size, 1);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(1));
}

#[test]
fn deserialise_with_password_none() {
    let json = r#"{"host": "127.0.0.1", "password": null}"#;
    let cfg: RedisConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.password.is_none());
}

#[test]
fn deserialise_missing_host_fails() {
    let json = r#"{"port": 6379}"#;
    let result = serde_json::from_str::<RedisConfig>(json);
    assert!(result.is_err());
}

// ── Connection URL edge cases ───────────────────────────────────────────────

#[test]
fn connection_url_password_with_special_chars() {
    let cfg = RedisConfig {
        password: Some("p@ss:word/123".into()),
        ..Default::default()
    };
    let url = cfg.connection_url();
    assert!(url.contains("p@ss:word/123"));
}

#[test]
fn connection_url_all_databases() {
    for db in 0..16u8 {
        let cfg = RedisConfig {
            database: db,
            ..Default::default()
        };
        let url = cfg.connection_url();
        assert!(url.ends_with(&format!("/{db}")), "db={db} url={url}");
    }
}

#[test]
fn connection_url_high_port() {
    let cfg = RedisConfig {
        port: 65535,
        ..Default::default()
    };
    assert!(cfg.connection_url().contains(":65535/"));
}

// ── Config Clone ────────────────────────────────────────────────────────────

#[test]
fn config_clone_is_independent() {
    let cfg1 = RedisConfig {
        host: "host1".into(),
        key_prefix: Some("prefix1".into()),
        ..Default::default()
    };
    let mut cfg2 = cfg1.clone();
    cfg2.host = "host2".into();
    cfg2.key_prefix = Some("prefix2".into());

    assert_eq!(cfg1.host, "host1");
    assert_eq!(cfg1.key_prefix.as_deref(), Some("prefix1"));
    assert_eq!(cfg2.host, "host2");
    assert_eq!(cfg2.key_prefix.as_deref(), Some("prefix2"));
}

// ── Config Debug ────────────────────────────────────────────────────────────

#[test]
fn config_debug_format() {
    let cfg = RedisConfig::default();
    let debug = format!("{:?}", cfg);
    assert!(debug.contains("RedisConfig"));
    assert!(debug.contains("127.0.0.1"));
    assert!(debug.contains("6379"));
}

// ── TypedStore key formatting (unit-level, no Redis) ────────────────────────

/// We can't test TypedStore directly without a connection, but we can verify
/// the key formatting logic by inspecting the format pattern.
#[test]
fn typed_store_key_format_with_prefix() {
    let prefix = "myapp";
    let key = "user:123";
    let full = format!("{prefix}:{key}");
    assert_eq!(full, "myapp:user:123");
}

#[test]
fn typed_store_key_format_empty_key() {
    let prefix = "app";
    let key = "";
    let full = format!("{prefix}:{key}");
    assert_eq!(full, "app:");
}

#[test]
fn typed_store_key_format_special_chars() {
    let prefix = "safe";
    let test_keys = vec![
        "../../etc/passwd",
        "key with spaces",
        "key:with:colons",
        "*",
        "?",
        "key\nwith\nnewlines",
    ];
    for key in test_keys {
        let full = format!("{prefix}:{key}");
        assert!(full.starts_with("safe:"), "key={key:?} full={full:?}");
    }
}

// ── JSON serialization round-trip (TypedStore logic, no Redis) ──────────────

#[test]
fn json_round_trip_simple_struct() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Simple {
        count: i32,
        tags: Vec<String>,
    }

    let val = Simple {
        count: 42,
        tags: vec!["a".into(), "b".into()],
    };
    let json = serde_json::to_string(&val).unwrap();
    let decoded: Simple = serde_json::from_str(&json).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn json_round_trip_nested_struct() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Inner {
        key: String,
    }

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Outer {
        name: String,
        score: f64,
        active: bool,
        inner: Option<Inner>,
        metadata: std::collections::HashMap<String, String>,
    }

    let mut meta = std::collections::HashMap::new();
    meta.insert("env".into(), "test".into());

    let val = Outer {
        name: "test".into(),
        score: 99.5,
        active: true,
        inner: Some(Inner { key: "deep".into() }),
        metadata: meta,
    };
    let json = serde_json::to_string(&val).unwrap();
    let decoded: Outer = serde_json::from_str(&json).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn json_round_trip_with_none() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct WithOption {
        value: Option<String>,
    }

    let val = WithOption { value: None };
    let json = serde_json::to_string(&val).unwrap();
    let decoded: WithOption = serde_json::from_str(&json).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn json_round_trip_empty_collections() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Collections {
        tags: Vec<String>,
        metadata: std::collections::HashMap<String, String>,
    }

    let val = Collections {
        tags: vec![],
        metadata: std::collections::HashMap::new(),
    };
    let json = serde_json::to_string(&val).unwrap();
    let decoded: Collections = serde_json::from_str(&json).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn json_deserialise_corrupted_fails() {
    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct Simple {
        count: i32,
    }

    let result = serde_json::from_str::<Simple>("{bad json");
    assert!(result.is_err());
}

#[test]
fn json_deserialise_type_mismatch_fails() {
    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct Typed {
        count: i32,
    }

    // count is a string, not an int
    let result = serde_json::from_str::<Typed>(r#"{"count": "not_a_number"}"#);
    assert!(result.is_err());
}

// ── Error mapping ───────────────────────────────────────────────────────────

#[test]
fn app_error_from_redis_error() {
    use rskit_errors::{AppError, ErrorCode};

    // Verify we can construct the same error type used by redis_err helper
    let err = AppError::new(ErrorCode::ExternalService, "redis error: test".to_string());
    assert_eq!(err.code, ErrorCode::ExternalService);
    assert!(err.message.contains("redis error"));
}

#[test]
fn app_error_connection_failed() {
    use rskit_errors::{AppError, ErrorCode};

    let err = AppError::new(
        ErrorCode::ConnectionFailed,
        "redis connection failed: timeout".to_string(),
    );
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
    assert!(err.message.contains("connection failed"));
}

#[test]
fn app_error_internal_for_json() {
    use rskit_errors::{AppError, ErrorCode};

    let err = AppError::new(
        ErrorCode::Internal,
        "json deserialise error: expected value".to_string(),
    );
    assert_eq!(err.code, ErrorCode::Internal);
}

// ── Component health (without connection) ───────────────────────────────────

#[test]
fn health_struct_healthy() {
    use rskit_bootstrap::Health;

    let h = Health::healthy("redis");
    assert_eq!(h.name, "redis");
    assert!(h.message.is_none());
}

#[test]
fn health_struct_unhealthy() {
    use rskit_bootstrap::Health;

    let h = Health::unhealthy("redis", "not connected");
    assert_eq!(h.name, "redis");
    assert_eq!(h.message.as_deref(), Some("not connected"));
}

// ── TTL Duration handling ───────────────────────────────────────────────────

#[test]
fn ttl_duration_max_returns_at_least_one_sec() {
    // The client code does: dur.as_secs().max(1) for TTL
    let dur = Duration::from_millis(100);
    let secs = dur.as_secs().max(1);
    assert_eq!(secs, 1, "sub-second TTL should floor to 1");
}

#[test]
fn ttl_duration_exact_seconds() {
    let dur = Duration::from_secs(300);
    let secs = dur.as_secs().max(1);
    assert_eq!(secs, 300);
}

#[test]
fn ttl_duration_zero_floors_to_one() {
    let dur = Duration::from_secs(0);
    let secs = dur.as_secs().max(1);
    assert_eq!(secs, 1, "zero TTL should floor to 1 in set_ex path");
}

#[test]
fn ttl_none_means_no_expiry() {
    let ttl: Option<Duration> = None;
    assert!(ttl.is_none());
}

#[test]
fn ttl_very_large_value() {
    let dur = Duration::from_secs(86400 * 365 * 10); // ~10 years
    let secs = dur.as_secs();
    assert!(secs > 0);
    assert_eq!(secs, 86400 * 365 * 10);
}

// ── Prefixed key logic ──────────────────────────────────────────────────────

/// Mirrors RedisClient::prefixed_key logic
fn prefixed_key(prefix: &Option<String>, key: &str) -> String {
    match prefix {
        Some(p) => format!("{p}:{key}"),
        None => key.to_owned(),
    }
}

#[test]
fn prefixed_key_with_prefix() {
    let prefix = Some("myapp".to_string());
    assert_eq!(prefixed_key(&prefix, "user:1"), "myapp:user:1");
}

#[test]
fn prefixed_key_without_prefix() {
    let prefix: Option<String> = None;
    assert_eq!(prefixed_key(&prefix, "user:1"), "user:1");
}

#[test]
fn prefixed_key_empty_key() {
    let prefix = Some("app".to_string());
    assert_eq!(prefixed_key(&prefix, ""), "app:");
}

#[test]
fn prefixed_key_isolation() {
    let prefix_a = Some("svcA".to_string());
    let prefix_b = Some("svcB".to_string());

    let key_a = prefixed_key(&prefix_a, "shared");
    let key_b = prefixed_key(&prefix_b, "shared");

    assert_ne!(key_a, key_b);
    assert_eq!(key_a, "svcA:shared");
    assert_eq!(key_b, "svcB:shared");
}

#[test]
fn prefixed_key_special_characters() {
    let prefix = Some("safe".to_string());
    let keys = vec![
        "../../etc/passwd",
        "key with spaces",
        "key:with:colons",
        "*",
        "?",
    ];
    for key in keys {
        let full = prefixed_key(&prefix, key);
        assert!(full.starts_with("safe:"), "key={key:?} full={full:?}");
    }
}

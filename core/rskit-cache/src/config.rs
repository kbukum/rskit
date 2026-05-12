use std::time::Duration;

use rskit_config::SecretString;
use serde::{Deserialize, Serialize};

/// Cache backend selection and common key settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Backend name looked up in an injected [`crate::CacheRegistry`].
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Optional prefix prepended to every key by backends that support it.
    pub key_prefix: Option<String>,
    /// In-memory backend options.
    #[serde(default)]
    pub memory: MemoryConfig,
    /// Redis backend options. Used only when the `redis` cargo feature is enabled.
    #[serde(default)]
    pub redis: RedisConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            key_prefix: None,
            memory: MemoryConfig::default(),
            redis: RedisConfig::default(),
        }
    }
}

fn default_backend() -> String {
    "memory".to_owned()
}

/// In-memory cache configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemoryConfig {
    /// Optional maximum entry count. `Some(0)` is normalized to unbounded by the memory backend.
    pub max_entries: Option<usize>,
}

/// Redis connection and pool configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    /// Redis server hostname or IP address.
    #[serde(default = "default_host")]
    pub host: String,
    /// Redis server port (default: 6379).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional password for Redis AUTH.
    pub password: Option<SecretString>,
    /// Redis database index (default: 0).
    #[serde(default)]
    pub database: u8,
    /// Connection pool size (default: 10).
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// Timeout for establishing a connection, represented as integer seconds.
    #[serde(default = "default_connect_timeout", with = "duration_seconds")]
    pub connect_timeout: Duration,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    6379
}

fn default_pool_size() -> u32 {
    10
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(5)
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            password: None,
            database: 0,
            pool_size: default_pool_size(),
            connect_timeout: default_connect_timeout(),
            key_prefix: None,
        }
    }
}

impl RedisConfig {
    /// Build the `redis://` connection URL from the config fields.
    #[must_use]
    pub fn connection_url(&self) -> String {
        match &self.password {
            Some(pw) => format!(
                "redis://:{}@{}:{}/{}",
                pw.expose(),
                self.host,
                self.port,
                self.database
            ),
            None => format!("redis://{}:{}/{}", self.host, self.port, self.database),
        }
    }
}

mod duration_seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }

    #[allow(dead_code)]
    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = CacheConfig::default();
        assert_eq!(cfg.backend, "memory");
        assert!(cfg.key_prefix.is_none());
        assert!(cfg.memory.max_entries.is_none());
    }

    #[test]
    fn connection_url_without_password() {
        let cfg = RedisConfig::default();
        assert_eq!(cfg.connection_url(), "redis://127.0.0.1:6379/0");
    }

    #[test]
    fn connection_url_with_password() {
        let cfg = RedisConfig {
            password: Some(SecretString::new("secret")),
            ..Default::default()
        };
        assert_eq!(cfg.connection_url(), "redis://:secret@127.0.0.1:6379/0");
    }

    #[test]
    fn connection_url_custom_host_port_db() {
        let cfg = RedisConfig {
            host: "redis.example.com".into(),
            port: 6380,
            database: 3,
            ..Default::default()
        };
        assert_eq!(cfg.connection_url(), "redis://redis.example.com:6380/3");
    }

    #[test]
    fn deserialise_from_json() {
        let json = r#"{"backend":"redis","redis":{"host":"localhost","port":6380,"database":2,"connect_timeout":10}}"#;
        let cfg: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.backend, "redis");
        assert_eq!(cfg.redis.host, "localhost");
        assert_eq!(cfg.redis.port, 6380);
        assert_eq!(cfg.redis.database, 2);
        assert_eq!(cfg.redis.connect_timeout, Duration::from_secs(10));
    }
}

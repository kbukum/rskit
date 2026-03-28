use std::time::Duration;

use serde::Deserialize;

/// Redis connection and pool configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    /// Redis server hostname or IP address.
    pub host: String,

    /// Redis server port (default: 6379).
    #[serde(default = "default_port")]
    pub port: u16,

    /// Optional password for Redis AUTH.
    pub password: Option<String>,

    /// Redis database index (default: 0).
    #[serde(default)]
    pub database: u8,

    /// Connection pool size (default: 10).
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// Timeout for establishing a connection.
    #[serde(default = "default_connect_timeout", with = "humantime_serde")]
    pub connect_timeout: Duration,

    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
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
            host: "127.0.0.1".into(),
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
    pub fn connection_url(&self) -> String {
        match &self.password {
            Some(pw) => format!(
                "redis://:{}@{}:{}/{}",
                pw, self.host, self.port, self.database
            ),
            None => format!("redis://{}:{}/{}", self.host, self.port, self.database),
        }
    }
}

/// Minimal serde helper that deserialises a [`Duration`] from a human-readable
/// string (e.g. `"5s"`, `"200ms"`) or an integer-seconds value, without
/// pulling in the full `humantime-serde` crate.
mod humantime_serde {
    use std::time::Duration;

    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
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
        let json = r#"{"host":"localhost","port":6380,"database":2,"connect_timeout":10}"#;
        let cfg: RedisConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 6380);
        assert_eq!(cfg.database, 2);
        assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
    }
}

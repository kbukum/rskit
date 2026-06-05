//! Redis adapter for [`rskit_cache`].

#![warn(missing_docs)]

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use rskit_cache::{CacheConfig, CacheRegistry, CacheStore, CacheStoreFactory};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_util::SecretString;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

/// Redis connection and pool configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct Config {
    /// Redis server hostname or IP address.
    #[serde(default = "default_host")]
    pub host: String,
    /// Redis server port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional password for Redis AUTH.
    pub password: Option<SecretString>,
    /// Redis database index.
    #[serde(default)]
    pub database: u8,
    /// Timeout for establishing a connection.
    #[serde(default = "default_connect_timeout", with = "duration_seconds")]
    pub connect_timeout: Duration,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database", &self.database)
            .field("connect_timeout", &self.connect_timeout)
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            password: None,
            database: 0,
            connect_timeout: default_connect_timeout(),
            key_prefix: None,
        }
    }
}

impl Config {
    fn connection_url(&self) -> String {
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

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    6379
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(5)
}

struct RedisClient {
    manager: redis::aio::ConnectionManager,
    config: Config,
}

impl RedisClient {
    async fn new(config: Config) -> AppResult<Self> {
        if config.connect_timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "redis connect_timeout must be greater than zero",
            ));
        }
        let client = redis::Client::open(config.connection_url()).map_err(|e| {
            AppError::new(ErrorCode::ConnectionFailed, "invalid redis connection URL").with_cause(e)
        })?;
        let manager = timeout(
            config.connect_timeout,
            redis::aio::ConnectionManager::new(client),
        )
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::ConnectionFailed,
                format!(
                    "redis connection timed out after {}ms",
                    config.connect_timeout.as_millis()
                ),
            )
        })?
        .map_err(redis_err)?;
        Ok(Self { manager, config })
    }

    fn prefixed_key(&self, key: &str) -> String {
        prefixed_key(key, self.config.key_prefix.as_deref())
    }

    fn conn(&self) -> redis::aio::ConnectionManager {
        self.manager.clone()
    }
}

#[async_trait::async_trait]
impl CacheStore for RedisClient {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        self.conn()
            .get(self.prefixed_key(key))
            .await
            .map_err(redis_err)
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let mut conn = self.conn();
        let key = self.prefixed_key(key);
        match ttl {
            Some(ttl) => {
                if ttl.is_zero() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        "cache TTL must be greater than zero",
                    ));
                }
                conn.pset_ex::<_, _, ()>(&key, val, redis_ttl_millis(ttl)?)
                    .await
                    .map_err(redis_err)
            }
            None => conn.set::<_, _, ()>(&key, val).await.map_err(redis_err),
        }
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let removed: i64 = self
            .conn()
            .del(self.prefixed_key(key))
            .await
            .map_err(redis_err)?;
        Ok(removed > 0)
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        self.conn()
            .exists(self.prefixed_key(key))
            .await
            .map_err(redis_err)
    }
}

struct RedisFactory {
    config: Config,
}

#[async_trait::async_trait]
impl CacheStoreFactory for RedisFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        let mut redis = self.config.clone();
        if redis.key_prefix.is_none() {
            redis.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(Arc::new(RedisClient::new(redis).await?))
    }
}

/// Explicitly register the Redis cache store.
pub fn register(registry: &mut CacheRegistry, config: Config) -> AppResult<()> {
    registry.register("redis", Arc::new(RedisFactory { config }))
}

fn redis_err(e: redis::RedisError) -> AppError {
    AppError::new(ErrorCode::ExternalService, format!("redis error: {e}")).with_cause(e)
}

fn redis_ttl_millis(ttl: Duration) -> AppResult<u64> {
    let millis = ttl.as_millis().max(1);
    u64::try_from(millis).map_err(|_| {
        AppError::new(
            ErrorCode::InvalidInput,
            "cache TTL is too large to represent safely for Redis",
        )
    })
}

fn prefixed_key(key: &str, prefix: Option<&str>) -> String {
    prefix.map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
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
    fn key_prefix_is_applied_when_configured() {
        assert_eq!(prefixed_key("user:1", Some("app")), "app:user:1");
        assert_eq!(prefixed_key("user:1", None), "user:1");
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
}

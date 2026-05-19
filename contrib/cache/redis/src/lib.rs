//! Redis adapter for [`rskit_cache`].

#![warn(missing_docs)]

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use rskit_cache::{CacheBackend, CacheConfig, CacheFactory, CacheRegistry};
use rskit_config::SecretString;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

/// Redis connection and pool configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct RedisConfig {
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

impl fmt::Debug for RedisConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database", &self.database)
            .field("connect_timeout", &self.connect_timeout)
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}

impl Default for RedisConfig {
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

impl RedisConfig {
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

/// Async Redis cache backend.
pub struct RedisClient {
    manager: redis::aio::ConnectionManager,
    config: RedisConfig,
}

impl RedisClient {
    /// Create a Redis cache backend.
    pub async fn new(config: RedisConfig) -> AppResult<Self> {
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
        self.config
            .key_prefix
            .as_ref()
            .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
    }

    fn conn(&self) -> redis::aio::ConnectionManager {
        self.manager.clone()
    }
}

#[async_trait::async_trait]
impl CacheBackend for RedisClient {
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
    config: RedisConfig,
}

#[async_trait::async_trait]
impl CacheFactory for RedisFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheBackend>> {
        let mut redis = self.config.clone();
        if redis.key_prefix.is_none() {
            redis.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(Arc::new(RedisClient::new(redis).await?))
    }
}

/// Explicitly register the Redis backend.
pub fn register_redis(registry: &mut CacheRegistry, config: RedisConfig) -> AppResult<()> {
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

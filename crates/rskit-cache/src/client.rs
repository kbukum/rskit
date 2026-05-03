use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use redis::AsyncCommands;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::RedisConfig;
use crate::registry::{CacheBackend, CacheFactory, CacheRegistry};

/// Async Redis client backed by [`redis::aio::ConnectionManager`].
///
/// Implements the rskit [`Component`] trait for lifecycle management.
pub struct RedisClient {
    manager: redis::aio::ConnectionManager,
    config: RedisConfig,
    connected: AtomicBool,
}

#[async_trait::async_trait]
impl CacheBackend for RedisClient {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        RedisClient::get(self, key).await
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        RedisClient::set(self, key, val, ttl).await
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        RedisClient::delete(self, key).await
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        RedisClient::exists(self, key).await
    }
}

struct RedisFactory;

#[async_trait::async_trait]
impl CacheFactory for RedisFactory {
    async fn create(
        &self,
        config: &crate::config::CacheConfig,
    ) -> AppResult<std::sync::Arc<dyn CacheBackend>> {
        let mut redis = config.redis.clone();
        if redis.key_prefix.is_none() {
            redis.key_prefix.clone_from(&config.key_prefix);
        }
        Ok(std::sync::Arc::new(RedisClient::new(redis).await?))
    }
}

/// Explicitly register the Redis backend. Requires the `redis` cargo feature.
pub fn register_redis(registry: &mut CacheRegistry) -> AppResult<()> {
    registry.register("redis", std::sync::Arc::new(RedisFactory))
}

impl RedisClient {
    /// Create a new [`RedisClient`] from the given configuration.
    ///
    /// Establishes a connection to Redis and verifies it with a PING.
    pub async fn new(config: RedisConfig) -> AppResult<Self> {
        let url = config.connection_url();
        let client = redis::Client::open(url.as_str()).map_err(|e| {
            AppError::new(
                ErrorCode::ConnectionFailed,
                format!("invalid redis URL: {e}"),
            )
            .with_cause(e)
        })?;

        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ConnectionFailed,
                    format!("redis connection failed: {e}"),
                )
                .with_cause(e)
            })?;

        tracing::debug!(host = %config.host, port = %config.port, db = %config.database, "redis connected");

        Ok(Self {
            manager,
            config,
            connected: AtomicBool::new(true),
        })
    }

    /// Returns a key with the configured prefix prepended.
    fn prefixed_key(&self, key: &str) -> String {
        match &self.config.key_prefix {
            Some(prefix) => format!("{prefix}:{key}"),
            None => key.to_owned(),
        }
    }

    /// Returns a clone of the underlying connection manager.
    fn conn(&self) -> redis::aio::ConnectionManager {
        // NOTE(#71 RS-ME-32): ConnectionManager uses Arc internally; cloning here is
        // cheap but avoid calling this inside tight loops — pre-clone outside the loop
        // or consider holding a Weak ref if you don't need to keep the manager alive.
        self.manager.clone()
    }

    // ── String operations ───────────────────────────────────────────────

    /// GET a string value by key.
    pub async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let k = self.prefixed_key(key);
        let val: Option<String> = self.conn().get(&k).await.map_err(redis_err)?;
        Ok(val)
    }

    /// SET a string value, with an optional TTL.
    pub async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let k = self.prefixed_key(key);
        let mut conn = self.conn();
        match ttl {
            Some(dur) => {
                if dur.is_zero() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        "cache TTL must be greater than zero",
                    ));
                }
                let millis = u64::try_from(dur.as_millis()).unwrap_or(u64::MAX).max(1);
                conn.pset_ex::<_, _, ()>(&k, val, millis)
                    .await
                    .map_err(redis_err)?;
            }
            None => {
                conn.set::<_, _, ()>(&k, val).await.map_err(redis_err)?;
            }
        }
        Ok(())
    }

    /// DELETE a key. Returns `true` if the key existed.
    pub async fn delete(&self, key: &str) -> AppResult<bool> {
        let k = self.prefixed_key(key);
        let removed: i64 = self.conn().del(&k).await.map_err(redis_err)?;
        Ok(removed > 0)
    }

    /// Check whether a key EXISTS.
    pub async fn exists(&self, key: &str) -> AppResult<bool> {
        let k = self.prefixed_key(key);
        let exists: bool = self.conn().exists(&k).await.map_err(redis_err)?;
        Ok(exists)
    }

    /// Set an EXPIRE (TTL) on an existing key.
    pub async fn expire(&self, key: &str, ttl: Duration) -> AppResult<bool> {
        let k = self.prefixed_key(key);
        let secs = ttl.as_secs() as i64;
        let set: bool = self.conn().expire(&k, secs).await.map_err(redis_err)?;
        Ok(set)
    }

    /// Get the remaining TTL for a key.
    ///
    /// Returns `None` if the key has no expiry or does not exist.
    pub async fn ttl(&self, key: &str) -> AppResult<Option<Duration>> {
        let k = self.prefixed_key(key);
        let seconds: i64 = redis::cmd("TTL")
            .arg(&k)
            .query_async(&mut self.conn())
            .await
            .map_err(redis_err)?;
        if seconds < 0 {
            Ok(None)
        } else {
            Ok(Some(Duration::from_secs(seconds as u64)))
        }
    }

    /// Atomically increment a key by `delta`. Returns the new value.
    pub async fn incr(&self, key: &str, delta: i64) -> AppResult<i64> {
        let k = self.prefixed_key(key);
        let val: i64 = self.conn().incr(&k, delta).await.map_err(redis_err)?;
        Ok(val)
    }

    // ── Hash operations ─────────────────────────────────────────────────

    /// HGET a field from a hash.
    pub async fn hget(&self, key: &str, field: &str) -> AppResult<Option<String>> {
        let k = self.prefixed_key(key);
        let val: Option<String> = self.conn().hget(&k, field).await.map_err(redis_err)?;
        Ok(val)
    }

    /// HSET a field in a hash.
    pub async fn hset(&self, key: &str, field: &str, val: &str) -> AppResult<()> {
        let k = self.prefixed_key(key);
        self.conn()
            .hset::<_, _, _, ()>(&k, field, val)
            .await
            .map_err(redis_err)?;
        Ok(())
    }

    /// HDEL a field from a hash. Returns `true` if the field existed.
    pub async fn hdel(&self, key: &str, field: &str) -> AppResult<bool> {
        let k = self.prefixed_key(key);
        let removed: i64 = self.conn().hdel(&k, field).await.map_err(redis_err)?;
        Ok(removed > 0)
    }

    /// HGETALL — retrieve all field-value pairs from a hash.
    pub async fn hgetall(&self, key: &str) -> AppResult<HashMap<String, String>> {
        let k = self.prefixed_key(key);
        let map: HashMap<String, String> = self.conn().hgetall(&k).await.map_err(redis_err)?;
        Ok(map)
    }

    // ── List operations ─────────────────────────────────────────────────

    /// LPUSH one or more values to the head of a list. Returns the new length.
    pub async fn lpush(&self, key: &str, vals: &[&str]) -> AppResult<i64> {
        let k = self.prefixed_key(key);
        let len: i64 = self.conn().lpush(&k, vals).await.map_err(redis_err)?;
        Ok(len)
    }

    /// RPUSH one or more values to the tail of a list. Returns the new length.
    pub async fn rpush(&self, key: &str, vals: &[&str]) -> AppResult<i64> {
        let k = self.prefixed_key(key);
        let len: i64 = self.conn().rpush(&k, vals).await.map_err(redis_err)?;
        Ok(len)
    }

    /// LRANGE — retrieve elements from a list by index range.
    pub async fn lrange(&self, key: &str, start: i64, stop: i64) -> AppResult<Vec<String>> {
        let k = self.prefixed_key(key);
        let items: Vec<String> = self
            .conn()
            .lrange(&k, start as isize, stop as isize)
            .await
            .map_err(redis_err)?;
        Ok(items)
    }

    /// LLEN — get the length of a list.
    pub async fn llen(&self, key: &str) -> AppResult<i64> {
        let k = self.prefixed_key(key);
        let len: i64 = self.conn().llen(&k).await.map_err(redis_err)?;
        Ok(len)
    }

    // ── Scan ────────────────────────────────────────────────────────────

    /// Iteratively SCAN for keys matching `pattern`.
    pub async fn scan(&self, pattern: &str) -> AppResult<Vec<String>> {
        let prefixed = self.prefixed_key(pattern);
        let mut conn = self.conn();
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&prefixed)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(keys)
    }

    // ── Pub/Sub ─────────────────────────────────────────────────────────

    /// PUBLISH a message to a channel.
    pub async fn publish(&self, channel: &str, msg: &str) -> AppResult<()> {
        self.conn()
            .publish::<_, _, ()>(channel, msg)
            .await
            .map_err(redis_err)?;
        Ok(())
    }
}

// ── Component implementation ────────────────────────────────────────────────

#[async_trait::async_trait]
impl Component for RedisClient {
    fn name(&self) -> &str {
        "redis"
    }

    async fn start(&self) -> AppResult<()> {
        // Verify the connection with PING
        let pong: String = redis::cmd("PING")
            .query_async(&mut self.conn())
            .await
            .map_err(redis_err)?;
        tracing::debug!(response = %pong, "redis PING ok");
        self.connected.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        tracing::debug!("redis client stopping");
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn health(&self) -> Health {
        if self.connected.load(Ordering::Relaxed) {
            Health::healthy("redis")
        } else {
            Health::unhealthy("redis", "not connected")
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Map a [`redis::RedisError`] to an [`AppError`].
fn redis_err(e: redis::RedisError) -> AppError {
    AppError::new(ErrorCode::ExternalService, format!("redis error: {e}")).with_cause(e)
}

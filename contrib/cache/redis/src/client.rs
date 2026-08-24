use std::time::Duration;

use redis::AsyncCommands;
use rskit_cache::CacheStore;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::time::timeout;

use super::Config;

pub(crate) struct RedisClient {
    manager: redis::aio::ConnectionManager,
    config: Config,
}

impl RedisClient {
    pub(crate) async fn new(config: Config) -> AppResult<Self> {
        if config.connect_timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "redis connect_timeout must be greater than zero",
            ));
        }
        if config.operation_timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "redis operation_timeout must be greater than zero",
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

    /// Bound a single Redis command by the configured per-operation timeout.
    async fn with_op_timeout<F, T>(&self, op: &str, future: F) -> AppResult<T>
    where
        F: std::future::Future<Output = redis::RedisResult<T>>,
    {
        match timeout(self.config.operation_timeout, future).await {
            Ok(result) => result.map_err(redis_err),
            Err(_) => Err(AppError::new(
                ErrorCode::Timeout,
                format!(
                    "redis {op} timed out after {}ms",
                    self.config.operation_timeout.as_millis()
                ),
            )),
        }
    }
}

#[async_trait::async_trait]
impl CacheStore for RedisClient {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let key = self.prefixed_key(key);
        self.with_op_timeout("get", async move { self.conn().get(key).await })
            .await
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let key = self.prefixed_key(key);
        match ttl {
            Some(ttl) => {
                if ttl.is_zero() {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        "cache TTL must be greater than zero",
                    ));
                }
                let millis = redis_ttl_millis(ttl)?;
                self.with_op_timeout("set", async move {
                    self.conn().pset_ex::<_, _, ()>(&key, val, millis).await
                })
                .await
            }
            None => {
                self.with_op_timeout("set", async move {
                    self.conn().set::<_, _, ()>(&key, val).await
                })
                .await
            }
        }
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let key = self.prefixed_key(key);
        let removed: i64 = self
            .with_op_timeout("delete", async move { self.conn().del(key).await })
            .await?;
        Ok(removed > 0)
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        let key = self.prefixed_key(key);
        self.with_op_timeout("exists", async move { self.conn().exists(key).await })
            .await
    }
}

pub(crate) fn redis_err(e: redis::RedisError) -> AppError {
    AppError::new(ErrorCode::ExternalService, format!("redis error: {e}")).with_cause(e)
}

pub(crate) fn redis_ttl_millis(ttl: Duration) -> AppResult<u64> {
    let millis = ttl.as_millis().max(1);
    u64::try_from(millis).map_err(|_| {
        AppError::new(
            ErrorCode::InvalidInput,
            "cache TTL is too large to represent safely for Redis",
        )
    })
}

pub(crate) fn prefixed_key(key: &str, prefix: Option<&str>) -> String {
    prefix.map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
}

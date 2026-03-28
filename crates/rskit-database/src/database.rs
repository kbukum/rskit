//! Database pool and [`Component`] implementation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use sqlx::any::{AnyPoolOptions, AnyQueryResult};
use sqlx::AnyPool;
use tracing::{error, info, warn};

use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::DatabaseConfig;

/// Async connection pool wrapping [`sqlx::AnyPool`].
///
/// Implements the [`Component`] trait for lifecycle management and health
/// reporting.  Queries that exceed the configured `slow_query_threshold` are
/// automatically logged at `WARN` level.
pub struct Database {
    pool: AnyPool,
    config: DatabaseConfig,
    connected: AtomicBool,
}

impl Database {
    /// Create a new [`Database`] from the given configuration.
    ///
    /// This installs the default sqlx `Any` drivers, builds the connection URL
    /// from `config`, and opens the pool.
    pub async fn new(config: DatabaseConfig) -> AppResult<Self> {
        sqlx::any::install_default_drivers();

        let url = config.connection_url();

        let pool = AnyPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.connect_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .connect(&url)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to create database pool");
                AppError::new(
                    ErrorCode::DatabaseError,
                    format!("failed to connect to database: {e}"),
                )
            })?;

        info!(
            driver = %config.driver,
            host = %config.host,
            port = config.port,
            database = %config.database,
            max_connections = config.max_connections,
            "database pool created"
        );

        Ok(Self {
            pool,
            config,
            connected: AtomicBool::new(true),
        })
    }

    /// Return a reference to the underlying [`AnyPool`].
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Execute a raw SQL statement, returning the query result.
    ///
    /// Queries that take longer than [`DatabaseConfig::slow_query_threshold`]
    /// are logged at `WARN` level.
    pub async fn execute(&self, query: &str) -> AppResult<AnyQueryResult> {
        let start = Instant::now();

        let result = sqlx::query(query).execute(&self.pool).await.map_err(|e| {
            error!(error = %e, query = %query, "query execution failed");
            AppError::new(ErrorCode::DatabaseError, format!("query failed: {e}"))
        })?;

        let elapsed = start.elapsed();
        if elapsed > self.config.slow_query_threshold {
            warn!(
                elapsed_ms = elapsed.as_millis() as u64,
                query = %query,
                "slow query detected"
            );
        }

        Ok(result)
    }
}

#[async_trait]
impl Component for Database {
    fn name(&self) -> &str {
        "database"
    }

    async fn start(&self) -> AppResult<()> {
        // Verify connectivity by acquiring a connection from the pool.
        self.pool.acquire().await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            error!(error = %e, "database ping failed during start");
            AppError::new(ErrorCode::DatabaseError, format!("database ping failed: {e}"))
        })?;

        self.connected.store(true, Ordering::SeqCst);
        info!("database component started");
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.connected.store(false, Ordering::SeqCst);
        self.pool.close().await;
        info!("database component stopped");
        Ok(())
    }

    fn health(&self) -> Health {
        if self.connected.load(Ordering::SeqCst) && !self.pool.is_closed() {
            Health::healthy("database")
        } else {
            Health::unhealthy("database", "pool is closed or disconnected")
        }
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("config", &self.config)
            .field("connected", &self.connected.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

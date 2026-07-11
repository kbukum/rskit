//! Vendor-neutral database client contracts and the in-memory backend.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use parking_lot::Mutex;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::config::{DatabaseConfig, MemoryDatabaseConfig};

/// Backend-neutral database statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseQuery {
    /// Statement text in the backend's query language.
    pub statement: String,
    /// Positional parameters encoded as backend-neutral JSON values.
    #[serde(default)]
    pub parameters: Vec<serde_json::Value>,
}

impl DatabaseQuery {
    /// Create a query without positional parameters.
    #[must_use]
    pub fn new(statement: impl Into<String>) -> Self {
        Self {
            statement: statement.into(),
            parameters: Vec::new(),
        }
    }

    /// Add one positional parameter.
    #[must_use]
    pub fn with_parameter(mut self, value: impl Into<serde_json::Value>) -> Self {
        self.parameters.push(value.into());
        self
    }
}

/// Backend-neutral database execution result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseResult {
    /// Rows affected by the statement, when the backend reports it.
    pub rows_affected: u64,
}

/// Active database transaction.
#[async_trait]
pub trait DatabaseTransaction: Send + Sync {
    /// Execute a statement inside the transaction.
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult>;
    /// Commit the transaction.
    async fn commit(self: Box<Self>) -> AppResult<()>;
    /// Roll back the transaction.
    async fn rollback(self: Box<Self>) -> AppResult<()>;
}

/// Vendor-neutral database client.
#[async_trait]
pub trait DatabaseClient: Send + Sync {
    /// Execute a statement.
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult>;
    /// Begin a transaction.
    async fn begin(&self) -> AppResult<Box<dyn DatabaseTransaction>>;
    /// Validate that the backend is usable.
    async fn ping(&self) -> AppResult<()>;
}

/// In-memory database backend for local development and tests.
#[derive(Debug)]
pub struct InMemoryDatabase {
    config: MemoryDatabaseConfig,
    connected: AtomicBool,
    history: Mutex<VecDeque<DatabaseQuery>>,
}

impl InMemoryDatabase {
    /// Create an in-memory database backend.
    #[must_use]
    pub fn new(config: MemoryDatabaseConfig) -> Self {
        Self {
            config,
            connected: AtomicBool::new(true),
            history: Mutex::new(VecDeque::new()),
        }
    }

    /// Return recorded statements retained by this backend.
    #[must_use]
    pub fn recorded_queries(&self) -> Vec<DatabaseQuery> {
        self.history.lock().iter().cloned().collect()
    }

    fn record(&self, query: DatabaseQuery) {
        if self.config.statement_history == 0 {
            return;
        }
        let mut history = self.history.lock();
        if history.len() == self.config.statement_history {
            history.pop_front();
        }
        history.push_back(query);
    }
}

impl Default for InMemoryDatabase {
    fn default() -> Self {
        Self::new(MemoryDatabaseConfig::default())
    }
}

#[async_trait]
impl DatabaseClient for InMemoryDatabase {
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::ConnectionFailed,
                "in-memory database is stopped",
            ));
        }
        if query.statement.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database query statement is required",
            ));
        }
        self.record(query);
        Ok(DatabaseResult { rows_affected: 1 })
    }

    async fn begin(&self) -> AppResult<Box<dyn DatabaseTransaction>> {
        self.ping().await?;
        Ok(Box::new(InMemoryTransaction {
            started_at: Instant::now(),
            queries: Mutex::new(Vec::new()),
            committed: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        }))
    }

    async fn ping(&self) -> AppResult<()> {
        if self.connected.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::ConnectionFailed,
                "in-memory database is stopped",
            ))
        }
    }
}

#[async_trait]
impl Component for InMemoryDatabase {
    fn name(&self) -> &str {
        "database"
    }

    async fn start(&self) -> AppResult<()> {
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn health(&self) -> Health {
        if self.connected.load(Ordering::SeqCst) {
            Health::healthy("database")
        } else {
            Health::unhealthy("database", "backend is stopped")
        }
    }
}

#[derive(Debug)]
struct InMemoryTransaction {
    started_at: Instant,
    queries: Mutex<Vec<DatabaseQuery>>,
    committed: AtomicBool,
    rolled_back: AtomicBool,
}

#[async_trait]
impl DatabaseTransaction for InMemoryTransaction {
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult> {
        if query.statement.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database query statement is required",
            ));
        }
        self.queries.lock().push(query);
        Ok(DatabaseResult { rows_affected: 1 })
    }

    async fn commit(self: Box<Self>) -> AppResult<()> {
        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database transaction was already rolled back",
            ));
        }
        self.committed.store(true, Ordering::SeqCst);
        let _elapsed = self.started_at.elapsed();
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> AppResult<()> {
        if self.committed.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database transaction was already committed",
            ));
        }
        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }
}

pub(crate) fn memory_from_config(config: &DatabaseConfig) -> InMemoryDatabase {
    InMemoryDatabase::new(config.memory.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use parking_lot::Mutex;
    use rskit_bootstrap::Component;

    use super::*;

    #[tokio::test]
    async fn in_memory_database_records_bounded_history_and_component_state() {
        let database = InMemoryDatabase::new(MemoryDatabaseConfig {
            name: "test".to_owned(),
            statement_history: 1,
        });

        assert_eq!(database.name(), "database");
        database
            .execute(DatabaseQuery::new("select 1").with_parameter(1))
            .await
            .unwrap();
        database
            .execute(DatabaseQuery::new("select 2"))
            .await
            .unwrap();
        let recorded = database.recorded_queries();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].statement, "select 2");
        assert!(database.health().is_healthy());

        database.stop().await.unwrap();
        assert_eq!(
            database.ping().await.unwrap_err().code(),
            ErrorCode::ConnectionFailed
        );
        assert_eq!(
            database
                .execute(DatabaseQuery::new("select 3"))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::ConnectionFailed
        );
        assert!(!database.health().is_healthy());

        database.start().await.unwrap();
        assert_eq!(
            database
                .execute(DatabaseQuery::new("   "))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn in_memory_transactions_validate_execute_and_guard_terminal_state() {
        let tx = InMemoryDatabase::default().begin().await.unwrap();
        assert_eq!(
            tx.execute(DatabaseQuery::new(" "))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            tx.execute(DatabaseQuery::new("insert"))
                .await
                .unwrap()
                .rows_affected,
            1
        );
        tx.commit().await.unwrap();

        let committed = Box::new(InMemoryTransaction {
            started_at: Instant::now(),
            queries: Mutex::new(Vec::new()),
            committed: AtomicBool::new(true),
            rolled_back: AtomicBool::new(false),
        });
        assert_eq!(
            committed.rollback().await.unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        let rolled_back = Box::new(InMemoryTransaction {
            started_at: Instant::now(),
            queries: Mutex::new(Vec::new()),
            committed: AtomicBool::new(false),
            rolled_back: AtomicBool::new(true),
        });
        assert_eq!(
            rolled_back.commit().await.unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        InMemoryDatabase::default()
            .begin()
            .await
            .unwrap()
            .rollback()
            .await
            .unwrap();
    }

    #[test]
    fn zero_statement_history_disables_recording() {
        let database = InMemoryDatabase::new(MemoryDatabaseConfig {
            name: "test".to_owned(),
            statement_history: 0,
        });

        database.record(DatabaseQuery::new("select"));

        assert!(database.recorded_queries().is_empty());
    }
}

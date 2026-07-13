//! `SQLite` backend implementing [`rskit_database::DatabaseClient`].

use std::sync::Arc;
use std::time::Duration;

use rskit_database::{
    DatabaseClient, DatabaseConfig, DatabaseFactory, DatabaseQuery, DatabaseRegistry,
    DatabaseResult, DatabaseTransaction,
};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, Executor, Pool, Sqlite};
use tokio::sync::Mutex;

/// Configuration for the `SQLite` database backend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// `SQLite` database URL, such as `sqlite://app.db` or `sqlite::memory:`.
    pub database_url: String,
    /// Maximum pooled connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum pooled connections.
    #[serde(default)]
    pub min_connections: u32,
    /// Connection acquisition timeout.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: Duration,
}

const fn default_max_connections() -> u32 {
    10
}

const fn default_connect_timeout() -> Duration {
    Duration::from_secs(30)
}

/// `SQLite` database client.
pub struct SqliteDatabase {
    pool: Pool<Sqlite>,
}

impl SqliteDatabase {
    /// Connect to `SQLite` using the provided config.
    pub async fn connect(config: Config) -> AppResult<Self> {
        validate_config(&config)?;
        let options = config
            .database_url
            .parse::<SqliteConnectOptions>()
            .map_err(|error| {
                AppError::new(ErrorCode::InvalidInput, "invalid `SQLite` database URL")
                    .with_cause(error)
            })?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.connect_timeout)
            .connect_with(options)
            .await
            .map_err(database_error("connect `SQLite` database"))?;
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl DatabaseClient for SqliteDatabase {
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult> {
        execute_on(&self.pool, query).await
    }

    async fn begin(&self) -> AppResult<Box<dyn DatabaseTransaction>> {
        let tx = self
            .pool
            .begin()
            .await
            .map_err(database_error("begin `SQLite` transaction"))?;
        Ok(Box::new(SqliteTransaction {
            tx: Mutex::new(Some(tx)),
        }))
    }

    async fn ping(&self) -> AppResult<()> {
        self.pool
            .acquire()
            .await
            .map_err(database_error("ping `SQLite` database"))?;
        Ok(())
    }
}

struct SqliteTransaction {
    tx: Mutex<Option<sqlx::Transaction<'static, Sqlite>>>,
}

#[async_trait::async_trait]
impl DatabaseTransaction for SqliteTransaction {
    #[allow(clippy::significant_drop_tightening)]
    async fn execute(&self, query: DatabaseQuery) -> AppResult<DatabaseResult> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "`SQLite` transaction is already closed",
            )
        })?;
        execute_on(&mut **tx, query).await
    }

    async fn commit(self: Box<Self>) -> AppResult<()> {
        let tx = self.tx.into_inner().ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "`SQLite` transaction is already closed",
            )
        })?;
        tx.commit()
            .await
            .map_err(database_error("commit `SQLite` transaction"))
    }

    async fn rollback(self: Box<Self>) -> AppResult<()> {
        let tx = self.tx.into_inner().ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "`SQLite` transaction is already closed",
            )
        })?;
        tx.rollback()
            .await
            .map_err(database_error("rollback `SQLite` transaction"))
    }
}

async fn execute_on<'e, E>(executor: E, query: DatabaseQuery) -> AppResult<DatabaseResult>
where
    E: Executor<'e, Database = Sqlite>,
{
    if query.statement.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "database query statement is required",
        ));
    }
    let mut sql = sqlx::query(AssertSqlSafe(query.statement.as_str()));
    for parameter in query.parameters {
        sql = bind_json_value(sql, parameter)?;
    }
    let result = sql
        .execute(executor)
        .await
        .map_err(database_error("execute `SQLite` statement"))?;
    Ok(DatabaseResult {
        rows_affected: result.rows_affected(),
    })
}

fn bind_json_value(
    query: sqlx::query::Query<'_, Sqlite, sqlx::sqlite::SqliteArguments>,
    value: serde_json::Value,
) -> AppResult<sqlx::query::Query<'_, Sqlite, sqlx::sqlite::SqliteArguments>> {
    match value {
        serde_json::Value::Null => Ok(query.bind(Option::<String>::None)),
        serde_json::Value::Bool(value) => Ok(query.bind(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(query.bind(value))
            } else if let Some(value) = value.as_u64() {
                let value = i64::try_from(value).map_err(|_| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        "`SQLite` integer parameter exceeds i64::MAX",
                    )
                })?;
                Ok(query.bind(value))
            } else if let Some(value) = value.as_f64() {
                Ok(query.bind(value))
            } else {
                Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "`SQLite` numeric parameter is not representable",
                ))
            }
        }
        serde_json::Value::String(value) => Ok(query.bind(value)),
        value @ (serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
            let text = serde_json::to_string(&value).map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    "`SQLite` structured parameter is not serializable to JSON text",
                )
                .with_cause(error)
            })?;
            Ok(query.bind(text))
        }
    }
}

fn validate_config(config: &Config) -> AppResult<()> {
    if config.database_url.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::MissingField,
            "`SQLite` database_url is required",
        ));
    }
    if config.max_connections == 0 {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "`SQLite` max_connections must be greater than zero",
        ));
    }
    if config.min_connections > config.max_connections {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "`SQLite` min_connections must not exceed max_connections",
        ));
    }
    if config.connect_timeout.is_zero() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "`SQLite` connect_timeout must be greater than zero",
        ));
    }
    Ok(())
}

fn database_error(operation: &'static str) -> impl FnOnce(sqlx::Error) -> AppError {
    move |error| {
        AppError::new(ErrorCode::DatabaseError, format!("{operation} failed")).with_cause(error)
    }
}

struct SqliteFactory {
    config: Config,
}

#[async_trait::async_trait]
impl DatabaseFactory for SqliteFactory {
    async fn create(&self, _config: &DatabaseConfig) -> AppResult<Arc<dyn DatabaseClient>> {
        Ok(Arc::new(
            SqliteDatabase::connect(self.config.clone()).await?,
        ))
    }
}

/// Explicitly register the `SQLite` database backend.
pub fn register(registry: &mut DatabaseRegistry, config: Config) -> AppResult<()> {
    registry.register("sqlite", Arc::new(SqliteFactory { config }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_database::{DatabaseClient, DatabaseConfig, DatabaseQuery, DatabaseRegistry};

    fn config() -> Config {
        Config {
            database_url: "sqlite::memory:".into(),
            max_connections: 1,
            min_connections: 0,
            connect_timeout: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn execute_binds_parameters_without_sql_concatenation() {
        let db = SqliteDatabase::connect(config()).await.unwrap();
        db.execute(DatabaseQuery::new("CREATE TABLE users (name TEXT)"))
            .await
            .unwrap();
        db.execute(
            DatabaseQuery::new("INSERT INTO users (name) VALUES (?)")
                .with_parameter("Robert'); DROP TABLE users;--"),
        )
        .await
        .unwrap();
        db.execute(
            DatabaseQuery::new("INSERT INTO users (name) VALUES (?)").with_parameter("Alice"),
        )
        .await
        .unwrap();
        assert_eq!(
            db.execute(
                DatabaseQuery::new("UPDATE users SET name = ? WHERE name = ?")
                    .with_parameter("Bob")
                    .with_parameter("Alice")
            )
            .await
            .unwrap()
            .rows_affected,
            1
        );
        db.execute(
            DatabaseQuery::new("INSERT INTO users (name) VALUES (?)")
                .with_parameter(serde_json::Value::Null),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn transaction_commit_and_rollback_are_explicit() {
        let db = SqliteDatabase::connect(config()).await.unwrap();
        db.execute(DatabaseQuery::new("CREATE TABLE items (name TEXT)"))
            .await
            .unwrap();
        let tx = db.begin().await.unwrap();
        tx.execute(DatabaseQuery::new("INSERT INTO items (name) VALUES (?)").with_parameter("one"))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let tx = db.begin().await.unwrap();
        tx.execute(DatabaseQuery::new("INSERT INTO items (name) VALUES (?)").with_parameter("two"))
            .await
            .unwrap();
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn structured_parameters_bind_as_json_text() {
        let db = SqliteDatabase::connect(config()).await.unwrap();
        db.execute(DatabaseQuery::new("CREATE TABLE docs (payload TEXT)"))
            .await
            .unwrap();
        db.execute(
            DatabaseQuery::new("INSERT INTO docs (payload) VALUES (?)")
                .with_parameter(serde_json::json!({"key": [1, 2, 3]})),
        )
        .await
        .unwrap();
    }

    #[test]
    fn config_validation_rejects_invalid_values() {
        assert_eq!(
            validate_config(&Config {
                database_url: String::new(),
                ..config()
            })
            .unwrap_err()
            .code(),
            ErrorCode::MissingField
        );
        assert_eq!(
            validate_config(&Config {
                max_connections: 0,
                ..config()
            })
            .unwrap_err()
            .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            validate_config(&Config {
                min_connections: 2,
                ..config()
            })
            .unwrap_err()
            .code(),
            ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn register_adds_backend_without_connecting() {
        let mut registry = DatabaseRegistry::new();
        register(&mut registry, config()).unwrap();
        assert!(registry.contains("sqlite"));
        let built = registry
            .build(&DatabaseConfig {
                backend: "sqlite".into(),
                ..DatabaseConfig::default()
            })
            .await
            .unwrap();
        built.ping().await.unwrap();
    }
}

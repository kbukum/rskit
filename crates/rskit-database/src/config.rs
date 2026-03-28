//! Database configuration types.

use std::time::Duration;

use serde::{Deserialize, Deserializer};
use validator::Validate;

/// Supported database drivers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbDriver {
    /// PostgreSQL.
    Postgres,
    /// MySQL / MariaDB.
    Mysql,
    /// SQLite (file or in-memory).
    Sqlite,
}

impl std::fmt::Display for DbDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbDriver::Postgres => f.write_str("postgres"),
            DbDriver::Mysql => f.write_str("mysql"),
            DbDriver::Sqlite => f.write_str("sqlite"),
        }
    }
}

/// TLS / SSL connection mode.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    /// Do not use TLS.
    Disable,
    /// Use TLS if the server supports it.
    Prefer,
    /// Require TLS; fail if unavailable.
    Require,
}

impl Default for SslMode {
    fn default() -> Self {
        Self::Prefer
    }
}

impl std::fmt::Display for SslMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SslMode::Disable => f.write_str("disable"),
            SslMode::Prefer => f.write_str("prefer"),
            SslMode::Require => f.write_str("require"),
        }
    }
}

/// Connection pool and query-logging configuration.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DatabaseConfig {
    /// Database driver to use.
    pub driver: DbDriver,
    /// Hostname or IP address of the database server.
    pub host: String,
    /// TCP port of the database server.
    pub port: u16,
    /// Username for authentication.
    pub user: String,
    /// Password for authentication.
    pub password: String,
    /// Database / schema name.
    pub database: String,
    /// Maximum number of connections in the pool (default: 10).
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum number of idle connections to maintain (default: 1).
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Timeout for establishing a new connection (default: 30s).
    #[serde(
        default = "default_connect_timeout",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub connect_timeout: Duration,
    /// Close idle connections after this duration.
    #[serde(default, deserialize_with = "deserialize_option_duration_secs")]
    pub idle_timeout: Option<Duration>,
    /// Maximum lifetime of a connection before it is recycled.
    #[serde(default, deserialize_with = "deserialize_option_duration_secs")]
    pub max_lifetime: Option<Duration>,
    /// Queries slower than this threshold are logged at WARN level (default: 1s).
    #[serde(
        default = "default_slow_query_threshold",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub slow_query_threshold: Duration,
    /// TLS connection mode (default: prefer).
    #[serde(default)]
    pub ssl_mode: SslMode,
}

impl DatabaseConfig {
    /// Build the database connection URL from the configuration fields.
    pub fn connection_url(&self) -> String {
        match self.driver {
            DbDriver::Postgres => {
                format!(
                    "postgres://{}:{}@{}:{}/{}?sslmode={}",
                    self.user, self.password, self.host, self.port, self.database, self.ssl_mode,
                )
            }
            DbDriver::Mysql => {
                format!(
                    "mysql://{}:{}@{}:{}/{}",
                    self.user, self.password, self.host, self.port, self.database,
                )
            }
            DbDriver::Sqlite => {
                format!("sqlite:{}", self.database)
            }
        }
    }
}

fn default_max_connections() -> u32 {
    10
}

fn default_min_connections() -> u32 {
    1
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_slow_query_threshold() -> Duration {
    Duration::from_secs(1)
}

fn deserialize_duration_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

fn deserialize_option_duration_secs<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<u64>::deserialize(deserializer)?;
    Ok(opt.map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_connection_url() {
        let cfg = DatabaseConfig {
            driver: DbDriver::Postgres,
            host: "localhost".into(),
            port: 5432,
            user: "admin".into(),
            password: "secret".into(),
            database: "mydb".into(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: None,
            max_lifetime: None,
            slow_query_threshold: Duration::from_secs(1),
            ssl_mode: SslMode::Disable,
        };
        assert_eq!(
            cfg.connection_url(),
            "postgres://admin:secret@localhost:5432/mydb?sslmode=disable"
        );
    }

    #[test]
    fn mysql_connection_url() {
        let cfg = DatabaseConfig {
            driver: DbDriver::Mysql,
            host: "db.example.com".into(),
            port: 3306,
            user: "root".into(),
            password: "pw".into(),
            database: "app".into(),
            max_connections: 5,
            min_connections: 1,
            connect_timeout: Duration::from_secs(10),
            idle_timeout: None,
            max_lifetime: None,
            slow_query_threshold: Duration::from_secs(2),
            ssl_mode: SslMode::Require,
        };
        assert_eq!(
            cfg.connection_url(),
            "mysql://root:pw@db.example.com:3306/app"
        );
    }

    #[test]
    fn sqlite_connection_url() {
        let cfg = DatabaseConfig {
            driver: DbDriver::Sqlite,
            host: String::new(),
            port: 0,
            user: String::new(),
            password: String::new(),
            database: ":memory:".into(),
            max_connections: 1,
            min_connections: 1,
            connect_timeout: Duration::from_secs(5),
            idle_timeout: None,
            max_lifetime: None,
            slow_query_threshold: Duration::from_secs(1),
            ssl_mode: SslMode::Disable,
        };
        assert_eq!(cfg.connection_url(), "sqlite::memory:");
    }

    #[test]
    fn ssl_mode_defaults_to_prefer() {
        assert_eq!(SslMode::default(), SslMode::Prefer);
    }

    #[test]
    fn db_driver_display() {
        assert_eq!(DbDriver::Postgres.to_string(), "postgres");
        assert_eq!(DbDriver::Mysql.to_string(), "mysql");
        assert_eq!(DbDriver::Sqlite.to_string(), "sqlite");
    }

    #[test]
    fn deserialize_config_from_json() {
        let json = r#"{
            "driver": "postgres",
            "host": "localhost",
            "port": 5432,
            "user": "admin",
            "password": "pass",
            "database": "testdb"
        }"#;
        let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.driver, DbDriver::Postgres);
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.min_connections, 1);
        assert_eq!(cfg.connect_timeout, Duration::from_secs(30));
        assert_eq!(cfg.slow_query_threshold, Duration::from_secs(1));
        assert_eq!(cfg.ssl_mode, SslMode::Prefer);
    }
}

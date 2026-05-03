use std::time::Duration;

#[cfg(feature = "sqlx-any")]
use rskit_database::SqlRepository;
use rskit_database::{DatabaseConfig, DatabaseRegistry, DbDriver, FindOpts, SslMode};

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
fn deserialize_config_from_json_with_defaults() {
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
    assert!(cfg.idle_timeout.is_none());
    assert!(cfg.max_lifetime.is_none());
}

// ── FindOpts builder ────────────────────────────────────────────────────────

#[test]
fn find_opts_defaults_are_empty() {
    let opts = FindOpts::default();
    assert!(opts.limit.is_none());
    assert!(opts.offset.is_none());
    assert!(opts.order_by.is_empty());
    assert!(opts.filters.is_empty());
}

#[test]
fn find_opts_with_limit_and_offset() {
    let opts = FindOpts::default().with_limit(20).with_offset(40);
    assert_eq!(opts.limit, Some(20));
    assert_eq!(opts.offset, Some(40));
}

#[test]
fn find_opts_order_by_chains() {
    let opts = FindOpts::default()
        .order_by("created_at DESC")
        .order_by("name ASC");
    assert_eq!(opts.order_by.len(), 2);
    assert_eq!(opts.order_by[0], "created_at DESC");
    assert_eq!(opts.order_by[1], "name ASC");
}

#[test]
fn find_opts_filter_adds_pairs() {
    let opts = FindOpts::default()
        .filter("status", "active")
        .filter("age", 25);
    assert_eq!(opts.filters.len(), 2);
    assert_eq!(opts.filters[0].0, "status");
    assert_eq!(opts.filters[0].1, serde_json::json!("active"));
    assert_eq!(opts.filters[1].0, "age");
    assert_eq!(opts.filters[1].1, serde_json::json!(25));
}

#[test]
fn find_opts_combined_builder() {
    let opts = FindOpts::default()
        .with_limit(10)
        .with_offset(0)
        .order_by("id")
        .filter("active", true);
    assert_eq!(opts.limit, Some(10));
    assert_eq!(opts.offset, Some(0));
    assert_eq!(opts.order_by, vec!["id".to_string()]);
    assert_eq!(opts.filters[0].1, serde_json::json!(true));
}

// ── Database (requires external service) ────────────────────────────────────

#[tokio::test]
#[cfg(feature = "sqlx-any")]
#[ignore = "requires running database server"]
async fn database_connects_to_sqlite_memory() {
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
    let db = rskit_database::Database::new(cfg).await.unwrap();
    assert!(!db.pool().is_closed());
}

#[test]
fn database_registry_empty_until_explicit_registration() {
    let registry = DatabaseRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.contains(&DbDriver::Postgres));
}

#[test]
fn database_registry_rejects_duplicate_driver_registration() {
    let mut registry = DatabaseRegistry::new();
    registry.register(DbDriver::Sqlite).unwrap();
    assert!(registry.register(DbDriver::Sqlite).is_err());
}

// ── Config validation and edge cases ────────────────────────────────────────

fn make_postgres_config() -> DatabaseConfig {
    DatabaseConfig {
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
        ssl_mode: SslMode::Prefer,
    }
}

#[test]
fn zero_max_connections_builds_valid_url() {
    let mut cfg = make_postgres_config();
    cfg.max_connections = 0;
    let url = cfg.connection_url();
    assert!(url.starts_with("postgres://"));
    assert!(url.contains("localhost:5432/mydb"));
}

#[test]
fn zero_min_connections_builds_valid_url() {
    let mut cfg = make_postgres_config();
    cfg.min_connections = 0;
    let url = cfg.connection_url();
    assert!(url.starts_with("postgres://"));
    assert!(url.contains("localhost:5432/mydb"));
}

#[test]
fn large_max_connections_builds_valid_url() {
    let mut cfg = make_postgres_config();
    cfg.max_connections = 10_000;
    let url = cfg.connection_url();
    assert!(url.starts_with("postgres://"));
    assert!(url.contains("localhost:5432/mydb"));
}

#[test]
fn postgres_url_with_ssl_require() {
    let mut cfg = make_postgres_config();
    cfg.ssl_mode = SslMode::Require;
    assert_eq!(
        cfg.connection_url(),
        "postgres://admin:secret@localhost:5432/mydb?sslmode=require"
    );
}

#[test]
fn postgres_url_with_ssl_prefer_default() {
    let cfg = make_postgres_config();
    assert_eq!(
        cfg.connection_url(),
        "postgres://admin:secret@localhost:5432/mydb?sslmode=prefer"
    );
}

#[test]
fn config_with_idle_timeout_and_max_lifetime() {
    let mut cfg = make_postgres_config();
    cfg.idle_timeout = Some(Duration::from_secs(300));
    cfg.max_lifetime = Some(Duration::from_secs(3600));
    assert_eq!(cfg.idle_timeout, Some(Duration::from_secs(300)));
    assert_eq!(cfg.max_lifetime, Some(Duration::from_secs(3600)));
    // URL generation still works regardless of pool options
    assert!(cfg.connection_url().starts_with("postgres://"));
}

// ── Config deserialization edge cases ───────────────────────────────────────

#[test]
fn deserialize_config_all_fields_explicit() {
    let json = r#"{
        "driver": "postgres",
        "host": "db.example.com",
        "port": 5433,
        "user": "root",
        "password": "s3cret",
        "database": "production",
        "max_connections": 50,
        "min_connections": 5,
        "connect_timeout": 60,
        "idle_timeout": 300,
        "max_lifetime": 3600,
        "slow_query_threshold": 5,
        "ssl_mode": "require"
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.driver, DbDriver::Postgres);
    assert_eq!(cfg.host, "db.example.com");
    assert_eq!(cfg.port, 5433);
    assert_eq!(cfg.user, "root");
    assert_eq!(cfg.password, "s3cret");
    assert_eq!(cfg.database, "production");
    assert_eq!(cfg.max_connections, 50);
    assert_eq!(cfg.min_connections, 5);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(60));
    assert_eq!(cfg.idle_timeout, Some(Duration::from_secs(300)));
    assert_eq!(cfg.max_lifetime, Some(Duration::from_secs(3600)));
    assert_eq!(cfg.slow_query_threshold, Duration::from_secs(5));
    assert_eq!(cfg.ssl_mode, SslMode::Require);
}

#[test]
fn deserialize_with_idle_and_max_lifetime_as_numbers() {
    let json = r#"{
        "driver": "postgres",
        "host": "localhost",
        "port": 5432,
        "user": "u",
        "password": "p",
        "database": "db",
        "idle_timeout": 120,
        "max_lifetime": 1800
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.idle_timeout, Some(Duration::from_secs(120)));
    assert_eq!(cfg.max_lifetime, Some(Duration::from_secs(1800)));
}

#[test]
fn deserialize_mysql_driver() {
    let json = r#"{
        "driver": "mysql",
        "host": "localhost",
        "port": 3306,
        "user": "root",
        "password": "pw",
        "database": "test"
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.driver, DbDriver::Mysql);
}

#[test]
fn deserialize_sqlite_driver() {
    let json = r#"{
        "driver": "sqlite",
        "host": "",
        "port": 0,
        "user": "",
        "password": "",
        "database": ":memory:"
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.driver, DbDriver::Sqlite);
    assert_eq!(cfg.connection_url(), "sqlite::memory:");
}

#[test]
fn deserialize_with_ssl_require() {
    let json = r#"{
        "driver": "postgres",
        "host": "h",
        "port": 5432,
        "user": "u",
        "password": "p",
        "database": "d",
        "ssl_mode": "require"
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.ssl_mode, SslMode::Require);
}

#[test]
fn deserialize_with_ssl_disable() {
    let json = r#"{
        "driver": "postgres",
        "host": "h",
        "port": 5432,
        "user": "u",
        "password": "p",
        "database": "d",
        "ssl_mode": "disable"
    }"#;
    let cfg: DatabaseConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.ssl_mode, SslMode::Disable);
}

#[test]
fn invalid_driver_deserialization_fails() {
    let json = r#"{
        "driver": "oracle",
        "host": "localhost",
        "port": 1521,
        "user": "u",
        "password": "p",
        "database": "d"
    }"#;
    let result = serde_json::from_str::<DatabaseConfig>(json);
    assert!(result.is_err());
}

// ── FindOpts edge cases ─────────────────────────────────────────────────────

#[test]
fn find_opts_with_limit_zero() {
    let opts = FindOpts::default().with_limit(0);
    assert_eq!(opts.limit, Some(0));
}

#[test]
fn find_opts_with_limit_negative() {
    let opts = FindOpts::default().with_limit(-1);
    assert_eq!(opts.limit, Some(-1));
}

#[test]
fn find_opts_with_offset_negative() {
    let opts = FindOpts::default().with_offset(-10);
    assert_eq!(opts.offset, Some(-10));
}

#[test]
fn find_opts_multiple_filters_same_column() {
    let opts = FindOpts::default()
        .filter("status", "active")
        .filter("status", "pending");
    assert_eq!(opts.filters.len(), 2);
    assert_eq!(opts.filters[0].0, "status");
    assert_eq!(opts.filters[0].1, serde_json::json!("active"));
    assert_eq!(opts.filters[1].0, "status");
    assert_eq!(opts.filters[1].1, serde_json::json!("pending"));
}

#[test]
fn find_opts_filter_various_json_types() {
    let opts = FindOpts::default()
        .filter("name", "alice")
        .filter("age", 30)
        .filter("active", true)
        .filter("deleted", serde_json::Value::Null)
        .filter("tags", serde_json::json!(["a", "b"]));
    assert_eq!(opts.filters.len(), 5);
    assert_eq!(opts.filters[0].1, serde_json::json!("alice"));
    assert_eq!(opts.filters[1].1, serde_json::json!(30));
    assert_eq!(opts.filters[2].1, serde_json::json!(true));
    assert_eq!(opts.filters[3].1, serde_json::Value::Null);
    assert_eq!(opts.filters[4].1, serde_json::json!(["a", "b"]));
}

#[test]
fn find_opts_empty_order_by_string() {
    let opts = FindOpts::default().order_by("");
    assert_eq!(opts.order_by.len(), 1);
    assert_eq!(opts.order_by[0], "");
}

#[test]
fn find_opts_large_limit_value() {
    let opts = FindOpts::default().with_limit(i64::MAX);
    assert_eq!(opts.limit, Some(i64::MAX));
}

#[test]
fn find_opts_order_by_preserves_insertion_order() {
    let opts = FindOpts::default()
        .order_by("z_col DESC")
        .order_by("a_col ASC")
        .order_by("m_col");
    assert_eq!(opts.order_by[0], "z_col DESC");
    assert_eq!(opts.order_by[1], "a_col ASC");
    assert_eq!(opts.order_by[2], "m_col");
}

// ── SqlRepository (requires Database, so ignored) ───────────────────────────

#[tokio::test]
#[cfg(feature = "sqlx-any")]
#[ignore = "requires running database server"]
async fn sql_repository_debug_format() {
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
    let db = std::sync::Arc::new(rskit_database::Database::new(cfg).await.unwrap());
    let repo = SqlRepository::<String>::new(db, "users");
    let debug = format!("{:?}", repo);
    assert!(debug.contains("SqlRepository"));
    assert!(debug.contains("users"));
}

#[tokio::test]
#[cfg(feature = "sqlx-any")]
#[ignore = "requires running database server"]
async fn sql_repository_table_name_returns_correct_value() {
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
    let db = std::sync::Arc::new(rskit_database::Database::new(cfg).await.unwrap());
    let repo = SqlRepository::<String>::new(db, "orders");
    assert_eq!(repo.table_name(), "orders");
}

// ── SslMode Display ─────────────────────────────────────────────────────────

#[test]
fn ssl_mode_display_disable() {
    assert_eq!(SslMode::Disable.to_string(), "disable");
}

#[test]
fn ssl_mode_display_prefer() {
    assert_eq!(SslMode::Prefer.to_string(), "prefer");
}

#[test]
fn ssl_mode_display_require() {
    assert_eq!(SslMode::Require.to_string(), "require");
}

// ── DbDriver equality ───────────────────────────────────────────────────────

#[test]
fn db_driver_same_are_equal() {
    assert_eq!(DbDriver::Postgres, DbDriver::Postgres);
    assert_eq!(DbDriver::Mysql, DbDriver::Mysql);
    assert_eq!(DbDriver::Sqlite, DbDriver::Sqlite);
}

#[test]
fn db_driver_different_are_not_equal() {
    assert_ne!(DbDriver::Postgres, DbDriver::Mysql);
    assert_ne!(DbDriver::Postgres, DbDriver::Sqlite);
    assert_ne!(DbDriver::Mysql, DbDriver::Sqlite);
}

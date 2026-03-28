use std::time::Duration;

use rskit_database::{DatabaseConfig, DbDriver, FindOpts, SslMode};

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

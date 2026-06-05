use std::num::NonZeroUsize;
use std::time::Duration;

use rskit_database::{
    DatabaseClient, DatabaseConfig, DatabaseQuery, DatabaseRegistry, FindOpts, InMemoryDatabase,
    MemoryDatabaseConfig, TenantScope, register_memory,
};

#[test]
fn config_defaults_to_memory_backend() {
    let cfg = DatabaseConfig::default();

    assert_eq!(cfg.backend, "memory");
    assert_eq!(cfg.memory.name, "default");
    assert_eq!(cfg.max_connections, 10);
    assert_eq!(cfg.min_connections, 1);
    assert_eq!(cfg.connect_timeout, Duration::from_secs(30));
}

#[tokio::test]
async fn registry_builds_memory_backend_from_config() {
    let mut registry = DatabaseRegistry::new();
    register_memory(&mut registry).unwrap();

    let db = registry.build(&DatabaseConfig::default()).await.unwrap();
    let result = db.execute(DatabaseQuery::new("SELECT 1")).await.unwrap();

    assert_eq!(result.rows_affected, 1);
}

#[tokio::test]
async fn unregistered_backend_returns_error() {
    let registry = DatabaseRegistry::new();

    let err = registry
        .build(&DatabaseConfig::default())
        .await
        .err()
        .unwrap();

    assert!(err.to_string().contains("not registered"));
}

#[tokio::test]
async fn memory_backend_rejects_empty_statement() {
    let db = InMemoryDatabase::new(MemoryDatabaseConfig::default());

    let err = db.execute(DatabaseQuery::new(" ")).await.err().unwrap();

    assert!(err.to_string().contains("statement is required"));
}

#[tokio::test]
async fn memory_backend_records_bounded_history() {
    let db = InMemoryDatabase::new(MemoryDatabaseConfig {
        name: "test".into(),
        statement_history: 1,
    });

    db.execute(DatabaseQuery::new("SELECT 1")).await.unwrap();
    db.execute(DatabaseQuery::new("SELECT 2")).await.unwrap();

    let history = db.recorded_queries();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].statement, "SELECT 2");
}

#[test]
fn find_opts_builder_sets_values() {
    let opts = FindOpts::default()
        .with_limit(10)
        .with_offset(20)
        .order_by("created_at DESC")
        .filter("status", "active");

    assert_eq!(opts.limit, Some(10));
    assert_eq!(opts.offset, Some(20));
    assert_eq!(opts.order_by, vec!["created_at DESC"]);
    assert_eq!(opts.filters.len(), 1);
}

#[test]
fn tenant_scope_builds_predicates() {
    let scope = TenantScope::new("workspace_id", "ws-1").unwrap();
    let first_param = NonZeroUsize::new(1).unwrap();

    assert_eq!(scope.where_clause(first_param), "workspace_id = $1");
    assert_eq!(
        scope.apply("SELECT * FROM tasks", first_param),
        "SELECT * FROM tasks WHERE workspace_id = $1"
    );
    assert_eq!(scope.value(), "ws-1");
}

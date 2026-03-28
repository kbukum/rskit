use std::sync::Arc;

use rskit_di::Container;

struct Config {
    db_url: String,
}

struct Logger {
    level: String,
}

// ── register (eager) and resolve ──────────────────────────────────────────────

#[test]
fn register_and_resolve_returns_correct_value() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "postgres://localhost/test".into(),
    }));

    let cfg = c.resolve::<Config>().unwrap();
    assert_eq!(cfg.db_url, "postgres://localhost/test");
}

#[test]
fn resolve_unregistered_type_returns_error() {
    let c = Container::new();
    let result = c.resolve::<Config>();
    assert!(result.is_err());
}

// ── multiple types ────────────────────────────────────────────────────────────

#[test]
fn multiple_types_in_same_container() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "postgres://db".into(),
    }));
    c.register(Arc::new(Logger {
        level: "info".into(),
    }));

    let cfg = c.resolve::<Config>().unwrap();
    let log = c.resolve::<Logger>().unwrap();

    assert_eq!(cfg.db_url, "postgres://db");
    assert_eq!(log.level, "info");
}

// ── Arc sharing (eager registration returns same instance) ────────────────────

#[test]
fn eager_resolve_returns_same_arc() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "shared".into(),
    }));

    let a = c.resolve::<Config>().unwrap();
    let b = c.resolve::<Config>().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

// ── factory (lazy) ────────────────────────────────────────────────────────────

#[test]
fn factory_creates_new_instance_each_resolve() {
    use std::sync::atomic::{AtomicU32, Ordering};

    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    struct Counter {
        id: u32,
    }

    let c = Container::new();
    c.register_factory(|| {
        let id = CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Counter { id }))
    });

    let a = c.resolve::<Counter>().unwrap();
    let b = c.resolve::<Counter>().unwrap();
    assert_ne!(a.id, b.id);
}

// ── singleton ─────────────────────────────────────────────────────────────────

#[test]
fn singleton_returns_same_instance() {
    use std::sync::atomic::{AtomicU32, Ordering};

    static SINGLETON_CALLS: AtomicU32 = AtomicU32::new(0);

    struct Service {
        id: u32,
    }

    let c = Container::new();
    c.register_singleton(|| {
        let id = SINGLETON_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Service { id }))
    });

    let a = c.resolve::<Service>().unwrap();
    let b = c.resolve::<Service>().unwrap();
    assert_eq!(a.id, b.id);
}

// ── is_registered ─────────────────────────────────────────────────────────────

#[test]
fn is_registered_reflects_state() {
    let c = Container::new();
    assert!(!c.is_registered::<Config>());

    c.register(Arc::new(Config { db_url: "x".into() }));
    assert!(c.is_registered::<Config>());
}

// ── close (async) ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn close_succeeds_on_empty_container() {
    let c = Container::new();
    c.close().await.unwrap();
}

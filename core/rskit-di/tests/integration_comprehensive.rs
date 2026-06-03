use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use rskit_di::{Closeable, Container};
use rskit_errors::{AppError, AppResult, ErrorCode};

// ── Helper types ──────────────────────────────────────────────────────────────

struct Config {
    db_url: String,
}

struct Logger {
    level: String,
}

struct Cache {
    name: String,
}

struct Metrics {
    prefix: String,
}

// ── 1. Concurrent singleton initialization race ──────────────────────────────

#[tokio::test]
async fn concurrent_singleton_init_returns_same_instance() {
    static INIT_COUNT: AtomicU32 = AtomicU32::new(0);

    struct SlowSvc {
        id: u32,
    }

    let c = Arc::new(Container::new());
    c.register_singleton(|| {
        let id = INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(10));
        Ok(Arc::new(SlowSvc { id }))
    });

    let mut handles = vec![];
    for _ in 0..20 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move { c2.resolve::<SlowSvc>() }));
    }

    let mut ids = vec![];
    for h in handles {
        let result = h.await.unwrap().unwrap();
        ids.push(result.id);
    }

    // All should get the same id
    let first = ids[0];
    for id in &ids {
        assert_eq!(*id, first, "all tasks should get same singleton instance");
    }
}

// ── 2. Factory returning errors ──────────────────────────────────────────────

#[test]
fn factory_error_propagates() {
    #[derive(Debug)]
    struct FailSvc;

    let c = Container::new();
    c.register_factory::<FailSvc, _>(|| Err(AppError::new(ErrorCode::Internal, "factory boom")));

    let result = c.resolve::<FailSvc>();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.message().contains("factory boom"));
}

#[test]
fn factory_error_does_not_cache() {
    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    struct MaybeSvc {
        val: u32,
    }

    let c = Container::new();
    c.register_factory(|| {
        let n = CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            Err(AppError::new(ErrorCode::Internal, "first call fails"))
        } else {
            Ok(Arc::new(MaybeSvc { val: n }))
        }
    });

    // First call fails
    assert!(c.resolve::<MaybeSvc>().is_err());
    // Second call succeeds
    let svc = c.resolve::<MaybeSvc>().unwrap();
    assert_eq!(svc.val, 1);
}

// ── 3. Arc contention under high concurrent access (50 tokio tasks) ──────────

#[tokio::test]
async fn high_concurrent_resolve_eager() {
    struct SharedSvc {
        val: i32,
    }

    let c = Arc::new(Container::new());
    c.register(Arc::new(SharedSvc { val: 42 }));

    let mut handles = vec![];
    for _ in 0..50 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let svc = c2.resolve::<SharedSvc>().unwrap();
            assert_eq!(svc.val, 42);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn high_concurrent_resolve_factory() {
    static FACTORY_CALLS: AtomicU32 = AtomicU32::new(0);

    #[allow(dead_code)]
    struct CountSvc {
        id: u32,
    }

    let c = Arc::new(Container::new());
    c.register_factory(|| {
        let id = FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(CountSvc { id }))
    });

    let mut handles = vec![];
    for _ in 0..50 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let _svc = c2.resolve::<CountSvc>().unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Factory should be called 50 times (once per resolve)
    assert_eq!(FACTORY_CALLS.load(Ordering::SeqCst), 50);
}

// ── 4. Closeable trait and close() ───────────────────────────────────────────

#[allow(dead_code)]
struct MockCloseable {
    closed: Mutex<bool>,
}

#[async_trait::async_trait]
impl Closeable for MockCloseable {
    async fn close(&self) -> AppResult<()> {
        *self.closed.lock() = true;
        Ok(())
    }
}

#[tokio::test]
async fn close_empty_container_succeeds() {
    let c = Container::new();
    c.close().await.unwrap();
}

#[tokio::test]
async fn close_idempotent() {
    let c = Container::new();
    c.close().await.unwrap();
    c.close().await.unwrap(); // second call should also succeed
}

// ── 5. Multiple types in same container ──────────────────────────────────────

#[test]
fn many_types_in_container() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "postgres://db".into(),
    }));
    c.register(Arc::new(Logger {
        level: "debug".into(),
    }));
    c.register(Arc::new(Cache {
        name: "redis".into(),
    }));
    c.register(Arc::new(Metrics {
        prefix: "app".into(),
    }));

    assert_eq!(c.resolve::<Config>().unwrap().db_url, "postgres://db");
    assert_eq!(c.resolve::<Logger>().unwrap().level, "debug");
    assert_eq!(c.resolve::<Cache>().unwrap().name, "redis");
    assert_eq!(c.resolve::<Metrics>().unwrap().prefix, "app");
}

// ── 6. Resolve unregistered type — error format ─────────────────────────────

#[test]
fn resolve_unregistered_error_contains_type_name() {
    #[derive(Debug)]
    struct UnknownSvc;

    let c = Container::new();
    let err = c.resolve::<UnknownSvc>().unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
    // Error message should contain the type name
    assert!(
        err.message().contains("UnknownSvc"),
        "error should mention type name, got: {}",
        err.message()
    );
}

// ── 7. Re-register same type overwrites ─────────────────────────────────────

#[test]
fn re_register_eager_overwrites() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "first".into(),
    }));
    c.register(Arc::new(Config {
        db_url: "second".into(),
    }));

    let cfg = c.resolve::<Config>().unwrap();
    assert_eq!(cfg.db_url, "second");
}

#[test]
fn re_register_singleton_overwrites() {
    static CTR: AtomicU32 = AtomicU32::new(0);

    struct VersionedSvc {
        version: u32,
    }

    let c = Container::new();
    c.register_singleton(|| {
        let v = CTR.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(VersionedSvc { version: v }))
    });

    let v1 = c.resolve::<VersionedSvc>().unwrap();
    assert_eq!(v1.version, 0);

    // Re-register resets the singleton
    c.register_singleton(|| Ok(Arc::new(VersionedSvc { version: 99 })));

    let v2 = c.resolve::<VersionedSvc>().unwrap();
    assert_eq!(v2.version, 99);
}

#[test]
fn re_register_factory_overwrites() {
    struct Svc {
        tag: String,
    }

    let c = Container::new();
    c.register_factory(|| Ok(Arc::new(Svc { tag: "old".into() })));
    c.register_factory(|| Ok(Arc::new(Svc { tag: "new".into() })));

    let svc = c.resolve::<Svc>().unwrap();
    assert_eq!(svc.tag, "new");
}

// ── 8. Large container (100+ types) ─────────────────────────────────────────

macro_rules! define_types {
    ($($name:ident),*) => {
        $(
            struct $name { #[allow(dead_code)] val: u32 }
        )*
    };
}

define_types!(
    T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39,
    T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58,
    T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77,
    T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96,
    T97, T98, T99
);

macro_rules! register_and_check {
    ($c:expr, $($name:ident => $val:expr),*) => {
        $(
            $c.register(Arc::new($name { val: $val }));
            assert!($c.is_registered::<$name>());
        )*
    };
}

#[test]
fn large_container_100_types() {
    let c = Container::new();
    register_and_check!(c,
        T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7, T8 => 8,
        T9 => 9, T10 => 10, T11 => 11, T12 => 12, T13 => 13, T14 => 14, T15 => 15, T16 => 16,
        T17 => 17, T18 => 18, T19 => 19, T20 => 20, T21 => 21, T22 => 22, T23 => 23, T24 => 24,
        T25 => 25, T26 => 26, T27 => 27, T28 => 28, T29 => 29, T30 => 30, T31 => 31, T32 => 32,
        T33 => 33, T34 => 34, T35 => 35, T36 => 36, T37 => 37, T38 => 38, T39 => 39, T40 => 40,
        T41 => 41, T42 => 42, T43 => 43, T44 => 44, T45 => 45, T46 => 46, T47 => 47, T48 => 48,
        T49 => 49, T50 => 50, T51 => 51, T52 => 52, T53 => 53, T54 => 54, T55 => 55, T56 => 56,
        T57 => 57, T58 => 58, T59 => 59, T60 => 60, T61 => 61, T62 => 62, T63 => 63, T64 => 64,
        T65 => 65, T66 => 66, T67 => 67, T68 => 68, T69 => 69, T70 => 70, T71 => 71, T72 => 72,
        T73 => 73, T74 => 74, T75 => 75, T76 => 76, T77 => 77, T78 => 78, T79 => 79, T80 => 80,
        T81 => 81, T82 => 82, T83 => 83, T84 => 84, T85 => 85, T86 => 86, T87 => 87, T88 => 88,
        T89 => 89, T90 => 90, T91 => 91, T92 => 92, T93 => 93, T94 => 94, T95 => 95, T96 => 96,
        T97 => 97, T98 => 98, T99 => 99
    );

    // Spot check a few resolves
    assert_eq!(c.resolve::<T0>().unwrap().val, 0);
    assert_eq!(c.resolve::<T50>().unwrap().val, 50);
    assert_eq!(c.resolve::<T99>().unwrap().val, 99);
}

// ── 9. Container is Send + Sync ─────────────────────────────────────────────

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn container_is_send_and_sync() {
    assert_send_sync::<Container>();
}

// ── 10. Default trait ────────────────────────────────────────────────────────

#[test]
fn container_default_creates_empty() {
    let c = Container::default();
    assert!(!c.is_registered::<Config>());
}

// ── 11. Singleton caches across resolves ─────────────────────────────────────

#[test]
fn singleton_arc_ptr_eq() {
    static SINGLETON_CTR: AtomicU32 = AtomicU32::new(0);

    #[allow(dead_code)]
    struct SingleSvc {
        id: u32,
    }

    let c = Container::new();
    c.register_singleton(|| {
        let id = SINGLETON_CTR.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(SingleSvc { id }))
    });

    let a = c.resolve::<SingleSvc>().unwrap();
    let b = c.resolve::<SingleSvc>().unwrap();
    assert!(Arc::ptr_eq(&a, &b), "singleton should return same Arc");
    assert_eq!(SINGLETON_CTR.load(Ordering::SeqCst), 1);
}

// ── 12. Factory creates distinct instances ───────────────────────────────────

#[test]
fn factory_creates_distinct_instances() {
    static FACTORY_CTR: AtomicU32 = AtomicU32::new(0);

    struct FactorySvc {
        id: u32,
    }

    let c = Container::new();
    c.register_factory(|| {
        let id = FACTORY_CTR.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FactorySvc { id }))
    });

    let a = c.resolve::<FactorySvc>().unwrap();
    let b = c.resolve::<FactorySvc>().unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert_ne!(a.id, b.id);
}

// ── 13. Eager registration returns same Arc ──────────────────────────────────

#[test]
fn eager_returns_same_arc_every_time() {
    let c = Container::new();
    let original = Arc::new(Config {
        db_url: "test".into(),
    });
    c.register(original.clone());

    let a = c.resolve::<Config>().unwrap();
    let b = c.resolve::<Config>().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(Arc::ptr_eq(&a, &original));
}

// ── 14. is_registered false for unregistered type ────────────────────────────

#[test]
fn is_registered_false_initially() {
    let c = Container::new();
    assert!(!c.is_registered::<Config>());
    assert!(!c.is_registered::<Logger>());
    assert!(!c.is_registered::<Cache>());
}

// ── 15. is_registered true after register ────────────────────────────────────

#[test]
fn is_registered_true_for_all_modes() {
    struct EagerSvc;
    struct FactorySvc;
    struct SingletonSvc;

    let c = Container::new();

    c.register(Arc::new(EagerSvc));
    assert!(c.is_registered::<EagerSvc>());

    c.register_factory(|| Ok(Arc::new(FactorySvc)));
    assert!(c.is_registered::<FactorySvc>());

    c.register_singleton(|| Ok(Arc::new(SingletonSvc)));
    assert!(c.is_registered::<SingletonSvc>());
}

// ── 16. Concurrent register and resolve ──────────────────────────────────────

#[tokio::test]
async fn concurrent_register_and_resolve() {
    let c = Arc::new(Container::new());
    c.register(Arc::new(Config {
        db_url: "base".into(),
    }));

    let mut handles = vec![];

    // Reader tasks
    for _ in 0..20 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let _cfg = c2.resolve::<Config>().unwrap();
        }));
    }

    // Writer tasks (registering different types)
    for i in 0..10u32 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            c2.register(Arc::new(Logger {
                level: format!("level-{i}"),
            }));
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ── 17. Factory closure captures state ───────────────────────────────────────

#[test]
fn factory_captures_external_state() {
    struct DbConn {
        url: String,
    }

    let url = "postgres://captured".to_string();
    let c = Container::new();
    c.register_factory(move || Ok(Arc::new(DbConn { url: url.clone() })));

    let conn = c.resolve::<DbConn>().unwrap();
    assert_eq!(conn.url, "postgres://captured");
}

// ── 18. Singleton factory error does not poison ──────────────────────────────

#[test]
fn singleton_factory_error_allows_retry() {
    static SINGLETON_ATTEMPT: AtomicU32 = AtomicU32::new(0);

    struct RetrySvc {
        val: u32,
    }

    let c = Container::new();
    c.register_singleton(|| {
        let n = SINGLETON_ATTEMPT.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            Err(AppError::new(ErrorCode::Internal, "first attempt fails"))
        } else {
            Ok(Arc::new(RetrySvc { val: n }))
        }
    });

    // First attempt — factory errors
    let r1 = c.resolve::<RetrySvc>();
    assert!(r1.is_err());

    // OnceLock was not set, so next call retries
    let r2 = c.resolve::<RetrySvc>();
    assert!(r2.is_ok());
    assert_eq!(r2.unwrap().val, 1);
}

// ── 19. Concrete type wrapping trait behavior ───────────────────────────────

struct EnglishGreeter;

impl EnglishGreeter {
    fn greet(&self) -> String {
        "Hello!".into()
    }
}

#[test]
fn concrete_service_in_container() {
    let c = Container::new();
    c.register(Arc::new(EnglishGreeter));

    let resolved = c.resolve::<EnglishGreeter>().unwrap();
    assert_eq!(resolved.greet(), "Hello!");
}

// ── 20. Arc sharing across threads ───────────────────────────────────────────

#[tokio::test]
async fn arc_shared_across_tasks() {
    struct SharedData {
        value: String,
    }

    let c = Arc::new(Container::new());
    c.register(Arc::new(SharedData {
        value: "shared".into(),
    }));

    let mut handles = vec![];
    for _ in 0..10 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let data = c2.resolve::<SharedData>().unwrap();
            assert_eq!(data.value, "shared");
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ── 21. Register overwrites with mode change ─────────────────────────────────

#[test]
fn eager_to_factory_overwrite() {
    struct ModeSvc {
        source: String,
    }

    let c = Container::new();
    c.register(Arc::new(ModeSvc {
        source: "eager".into(),
    }));
    assert_eq!(c.resolve::<ModeSvc>().unwrap().source, "eager");

    c.register_factory(|| {
        Ok(Arc::new(ModeSvc {
            source: "factory".into(),
        }))
    });
    assert_eq!(c.resolve::<ModeSvc>().unwrap().source, "factory");
}

#[test]
fn factory_to_singleton_overwrite() {
    struct ModeSvc2 {
        source: String,
    }

    let c = Container::new();
    c.register_factory(|| {
        Ok(Arc::new(ModeSvc2 {
            source: "factory".into(),
        }))
    });
    assert_eq!(c.resolve::<ModeSvc2>().unwrap().source, "factory");

    c.register_singleton(|| {
        Ok(Arc::new(ModeSvc2 {
            source: "singleton".into(),
        }))
    });
    assert_eq!(c.resolve::<ModeSvc2>().unwrap().source, "singleton");
}

// ── 22. Concurrent singleton + factory mix ───────────────────────────────────

#[tokio::test]
async fn concurrent_mixed_singleton_and_factory() {
    static MIX_FACTORY_CALLS: AtomicU32 = AtomicU32::new(0);
    static MIX_SINGLETON_CALLS: AtomicU32 = AtomicU32::new(0);

    #[allow(dead_code)]
    struct FactoryItem {
        id: u32,
    }
    #[allow(dead_code)]
    struct SingletonItem {
        id: u32,
    }

    let c = Arc::new(Container::new());
    c.register_factory(|| {
        let id = MIX_FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FactoryItem { id }))
    });
    c.register_singleton(|| {
        let id = MIX_SINGLETON_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(SingletonItem { id }))
    });

    let mut handles = vec![];
    for i in 0..30 {
        let c2 = Arc::clone(&c);
        if i % 2 == 0 {
            handles.push(tokio::spawn(
                async move { c2.resolve::<FactoryItem>().unwrap() },
            ));
        } else {
            handles.push(tokio::spawn(async move {
                let _ = c2.resolve::<SingletonItem>().unwrap();
                // Return FactoryItem-shaped Arc for uniform handle type — just resolve factory too
                c2.resolve::<FactoryItem>().unwrap()
            }));
        }
    }

    for h in handles {
        h.await.unwrap();
    }

    // Singleton should be called exactly once
    assert_eq!(MIX_SINGLETON_CALLS.load(Ordering::SeqCst), 1);
    // Factory should be called multiple times
    assert!(MIX_FACTORY_CALLS.load(Ordering::SeqCst) > 1);
}

// ── 23. Empty container operations ───────────────────────────────────────────

#[test]
fn empty_container_resolve_returns_not_found() {
    #[derive(Debug)]
    struct Anything;
    let c = Container::new();
    let err = c.resolve::<Anything>().unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
}

#[test]
fn empty_container_is_registered_false() {
    struct Anything;
    let c = Container::new();
    assert!(!c.is_registered::<Anything>());
}

// ── 24. Singleton with expensive init ────────────────────────────────────────

#[test]
fn singleton_expensive_init_called_once() {
    static EXPENSIVE_CALLS: AtomicU32 = AtomicU32::new(0);

    struct ExpensiveSvc {
        data: Vec<u8>,
    }

    let c = Container::new();
    c.register_singleton(|| {
        EXPENSIVE_CALLS.fetch_add(1, Ordering::SeqCst);
        // Simulate expensive computation
        let data = vec![0u8; 1024];
        Ok(Arc::new(ExpensiveSvc { data }))
    });

    for _ in 0..10 {
        let svc = c.resolve::<ExpensiveSvc>().unwrap();
        assert_eq!(svc.data.len(), 1024);
    }

    assert_eq!(EXPENSIVE_CALLS.load(Ordering::SeqCst), 1);
}

// ── 25. Register after resolve returns updated value ─────────────────────────

#[test]
fn register_after_resolve_returns_new_value() {
    let c = Container::new();
    c.register(Arc::new(Config {
        db_url: "old".into(),
    }));
    assert_eq!(c.resolve::<Config>().unwrap().db_url, "old");

    c.register(Arc::new(Config {
        db_url: "new".into(),
    }));
    assert_eq!(c.resolve::<Config>().unwrap().db_url, "new");
}

// ── 26. Factory with complex closure ─────────────────────────────────────────

#[test]
fn factory_with_counter_closure() {
    let counter = Arc::new(AtomicU32::new(0));

    struct SeqSvc {
        seq: u32,
    }

    let c = Container::new();
    let counter_clone = Arc::clone(&counter);
    c.register_factory(move || {
        let seq = counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(SeqSvc { seq }))
    });

    let a = c.resolve::<SeqSvc>().unwrap();
    let b = c.resolve::<SeqSvc>().unwrap();
    let cc = c.resolve::<SeqSvc>().unwrap();

    assert_eq!(a.seq, 0);
    assert_eq!(b.seq, 1);
    assert_eq!(cc.seq, 2);
}

// ── 27. Container with many concurrent singleton resolutions ─────────────────

#[tokio::test]
async fn stress_concurrent_singleton() {
    static STRESS_CTR: AtomicU32 = AtomicU32::new(0);

    struct StressSvc {
        val: u32,
    }

    let c = Arc::new(Container::new());
    c.register_singleton(|| {
        STRESS_CTR.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(StressSvc { val: 42 }))
    });

    let mut handles = vec![];
    for _ in 0..100 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let svc = c2.resolve::<StressSvc>().unwrap();
            assert_eq!(svc.val, 42);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Singleton factory called exactly once
    assert_eq!(STRESS_CTR.load(Ordering::SeqCst), 1);
}

// ── 28. ZST (zero-sized type) in container ───────────────────────────────────

#[test]
fn zero_sized_type_works() {
    struct Marker;

    let c = Container::new();
    c.register(Arc::new(Marker));
    assert!(c.is_registered::<Marker>());
    let _ = c.resolve::<Marker>().unwrap();
}

// ── 29. String and primitive wrappers ────────────────────────────────────────

#[test]
fn string_wrapper_in_container() {
    struct AppName(String);

    let c = Container::new();
    c.register(Arc::new(AppName("my-app".into())));
    let name = c.resolve::<AppName>().unwrap();
    assert_eq!(name.0, "my-app");
}

// ── 30. Concurrent close is safe ─────────────────────────────────────────────

#[tokio::test]
async fn concurrent_close_is_safe() {
    let c = Arc::new(Container::new());
    c.register(Arc::new(Config {
        db_url: "test".into(),
    }));

    let mut handles = vec![];
    for _ in 0..10 {
        let c2 = Arc::clone(&c);
        handles.push(tokio::spawn(async move {
            let _ = c2.close().await;
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

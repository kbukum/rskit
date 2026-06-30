//! Extended integration tests for rskit-worker: edge cases, concurrency, events.

use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_worker::{
    Event, EventKind, Handler, Pool, PoolConfig, Progress, ResourceRequirements,
    RoundRobinDispatcher, WorkerScheduler, WorkloadBatch, WorkloadConfig, WorkloadScheduler,
    WorkloadSpec,
};

// ── Shared test handlers ──────────────────────────────────────────────────────

struct DoubleHandler;

#[async_trait::async_trait]
impl Handler<i32, i32> for DoubleHandler {
    async fn handle(
        &self,
        task: i32,
        _emit: mpsc::Sender<Event<i32>>,
        _cancel: CancellationToken,
    ) -> AppResult<i32> {
        Ok(task * 2)
    }
}

struct ErrorHandler;

#[async_trait::async_trait]
impl Handler<i32, i32> for ErrorHandler {
    async fn handle(
        &self,
        _task: i32,
        _emit: mpsc::Sender<Event<i32>>,
        _cancel: CancellationToken,
    ) -> AppResult<i32> {
        Err(AppError::new(ErrorCode::Internal, "intentional error"))
    }
}

struct PendingHandler {
    started: Arc<Notify>,
}

#[async_trait::async_trait]
impl Handler<i32, i32> for PendingHandler {
    async fn handle(
        &self,
        _task: i32,
        _emit: mpsc::Sender<Event<i32>>,
        cancel: CancellationToken,
    ) -> AppResult<i32> {
        self.started.notify_one();
        tokio::select! {
            () = pending() => unreachable!("pending future never completes"),
            _ = cancel.cancelled() => Err(AppError::new(ErrorCode::Internal, "cancelled")),
        }
    }
}

struct EventEmittingHandler;

#[async_trait::async_trait]
impl Handler<i32, i32> for EventEmittingHandler {
    async fn handle(
        &self,
        task: i32,
        emit: mpsc::Sender<Event<i32>>,
        _cancel: CancellationToken,
    ) -> AppResult<i32> {
        let id = uuid::Uuid::new_v4();
        // Emit progress
        let p = Progress::new(1, Some(2)).with_message("step 1");
        let _ = emit.send(Event::progress(id, "test-worker", p)).await;
        // Emit partial
        let _ = emit.send(Event::partial(id, "test-worker", task)).await;
        // Emit log
        let _ = emit.send(Event::log(id, "test-worker", "doing work")).await;
        Ok(task * 2)
    }
}

struct CountingHandler {
    counter: Arc<AtomicU32>,
}

struct IdleHandler;

struct StubbornHandler;

#[async_trait::async_trait]
impl Handler<i32, i32> for IdleHandler {
    async fn handle(
        &self,
        task: i32,
        _emit: mpsc::Sender<Event<i32>>,
        _cancel: CancellationToken,
    ) -> AppResult<i32> {
        Ok(task)
    }
}

#[async_trait::async_trait]
impl Handler<i32, i32> for StubbornHandler {
    async fn handle(
        &self,
        _task: i32,
        _emit: mpsc::Sender<Event<i32>>,
        _cancel: CancellationToken,
    ) -> AppResult<i32> {
        pending::<AppResult<i32>>().await
    }
}

#[async_trait::async_trait]
impl Handler<u32, u32> for CountingHandler {
    async fn handle(
        &self,
        task: u32,
        _emit: mpsc::Sender<Event<u32>>,
        _cancel: CancellationToken,
    ) -> AppResult<u32> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(task)
    }
}

// ── 1. Pool creation with custom PoolConfig ───────────────────────────────────

#[tokio::test]
async fn pool_creation_with_custom_config() {
    let config = PoolConfig::new("custom")
        .with_size(2)
        .with_queue_size(64)
        .with_grace_period(Duration::from_secs(5));
    let pool = Pool::new(Arc::new(DoubleHandler), config);

    let stats = pool.stats();
    assert_eq!(stats.name, "custom");
    assert_eq!(stats.capacity, 2);
    assert_eq!(stats.running, 0);
}

// ── 2. Submit and await multiple results ──────────────────────────────────────

#[tokio::test]
async fn submit_and_await_multiple_results() {
    let pool = Pool::new(
        Arc::new(DoubleHandler),
        PoolConfig::new("multi").with_size(4),
    );

    let mut handles = Vec::new();
    for i in 1..=5 {
        handles.push(pool.submit(i).await.unwrap());
    }

    for (i, h) in handles.into_iter().enumerate() {
        let result = h.result().await.unwrap();
        assert_eq!(result, (i as i32 + 1) * 2);
    }
}

// ── 3. Error-producing task propagates error ──────────────────────────────────

#[tokio::test]
async fn error_task_propagates() {
    let pool = Pool::new(Arc::new(ErrorHandler), PoolConfig::new("err").with_size(1));
    let handle = pool.submit(1).await.unwrap();
    let result = handle.result().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("intentional error"));
}

// ── 4. Pool shutdown during active tasks ──────────────────────────────────────

#[tokio::test]
async fn pool_shutdown_during_active_tasks() {
    let started = Arc::new(Notify::new());
    let pool = Pool::new(
        Arc::new(PendingHandler {
            started: started.clone(),
        }),
        PoolConfig::new("shutdown-active")
            .with_size(2)
            .with_grace_period(Duration::from_millis(1)),
    );

    let started_task = started.notified();
    let h1 = pool.submit(1).await.unwrap();
    let cancel_token = h1.cancel_token();
    let _h2 = pool.submit(2).await.unwrap();

    timeout(Duration::from_secs(2), started_task)
        .await
        .expect("worker task should start");

    timeout(Duration::from_secs(2), pool.shutdown())
        .await
        .expect("pool shutdown must not hang")
        .unwrap();

    // The cancel token should be usable (handler checks it)
    cancel_token.cancel();
}

// Regression: a task whose envelope was dequeued by the runner but had not yet
// acquired a permit when shutdown fired must surface a `ServiceUnavailable`
// error to the awaiting handle, not silently drop the oneshot sender.
#[tokio::test]
async fn pool_shutdown_fails_dequeued_but_unscheduled_task() {
    let started = Arc::new(Notify::new());
    let pool = Pool::new(
        Arc::new(PendingHandler {
            started: started.clone(),
        }),
        PoolConfig::new("shutdown-dequeued")
            .with_size(1)
            .with_queue_size(1)
            .with_grace_period(Duration::from_millis(100)),
    );

    // First submission occupies the only permit.
    let started_task = started.notified();
    let _h1 = pool.submit(1).await.unwrap();
    timeout(Duration::from_secs(2), started_task)
        .await
        .expect("first worker task should start");
    // Second is queued and will be dequeued by the runner, but cannot acquire a
    // permit until h1 finishes; we shut down before that ever happens.
    let h2 = pool.submit(2).await.unwrap();

    // With queue capacity one, a third submit can complete only after h2 has
    // been dequeued by the runner and is waiting for a permit.
    let _h3 = timeout(Duration::from_secs(2), pool.submit(3))
        .await
        .expect("runner should dequeue h2 and free queue capacity")
        .unwrap();
    timeout(Duration::from_secs(2), pool.shutdown())
        .await
        .expect("pool shutdown must not hang")
        .unwrap();

    let err = timeout(Duration::from_secs(2), h2.result())
        .await
        .expect("handle.result() must not hang after shutdown")
        .expect_err("dequeued task must fail with ServiceUnavailable");
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
}

// A configured size of 0 would create a zero-permit semaphore that blocks
// every submission, so Pool::new clamps the size up to at least 1.
#[tokio::test]
async fn pool_with_zero_size_clamps_to_one() {
    let pool = Pool::new(
        Arc::new(DoubleHandler),
        PoolConfig::new("size-zero").with_size(0),
    );
    let h = pool.submit(21).await.unwrap();
    let v = timeout(Duration::from_secs(2), h.result())
        .await
        .expect("must not hang")
        .expect("ok");
    assert_eq!(v, 42);
    pool.shutdown().await.unwrap();
}

#[tokio::test]
async fn reject_policy_reports_shutdown_as_service_unavailable() {
    let pool = Pool::new(
        Arc::new(DoubleHandler),
        PoolConfig::new("reject-closed").with_overflow_policy(rskit_worker::OverflowPolicy::Reject),
    );
    pool.close();

    let err = match pool.submit(1).await {
        Ok(_) => panic!("closed pool must reject as ServiceUnavailable"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
}

#[tokio::test]
async fn shutdown_respects_grace_period() {
    let pool = Pool::new(
        Arc::new(StubbornHandler),
        PoolConfig::new("shutdown-grace")
            .with_size(1)
            .with_grace_period(Duration::from_millis(1)),
    );
    let _handle = pool.submit(1).await.unwrap();

    timeout(Duration::from_millis(200), pool.shutdown())
        .await
        .expect("shutdown should return within configured grace period")
        .unwrap();
}

// ── 5. Task cancellation ──────────────────────────────────────────────────────

#[tokio::test]
async fn task_cancellation() {
    let started = Arc::new(Notify::new());
    let pool = Pool::new(
        Arc::new(PendingHandler {
            started: started.clone(),
        }),
        PoolConfig::new("cancel").with_size(1),
    );

    let started_task = started.notified();
    let handle = pool.submit(42).await.unwrap();
    started_task.await;

    handle.cancel();
    let result = handle.result().await;
    assert!(result.is_err(), "cancelled task should return error");
}

// ── 6. TaskHandle events collection ───────────────────────────────────────────

#[tokio::test]
async fn task_handle_events_collection() {
    let pool = Pool::new(
        Arc::new(EventEmittingHandler),
        PoolConfig::new("events").with_size(1),
    );

    let handle = pool.submit(5).await.unwrap();
    let task_id = handle.id;
    let mut rx = handle.events();

    let result = handle.result().await.unwrap();
    assert_eq!(result, 10);

    // Collect broadcast events
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }

    // Should have at least the handler-emitted events + final result event
    assert!(
        !events.is_empty(),
        "expected at least some events, got none"
    );

    // At least one event should reference this task
    let has_task_event = events.iter().any(|e| e.task_id == task_id);
    assert!(has_task_event, "expected event with matching task_id");
}

// ── 7. PoolStats accuracy ─────────────────────────────────────────────────────

#[tokio::test]
async fn pool_stats_accuracy() {
    let started = Arc::new(Notify::new());
    let pool = Pool::new(
        Arc::new(PendingHandler {
            started: started.clone(),
        }),
        PoolConfig::new("stats").with_size(4),
    );

    // Before any submissions
    let stats = pool.stats();
    assert_eq!(stats.running, 0);
    assert_eq!(stats.capacity, 4);
    assert_eq!(pool.available_permits(), 4);

    // Submit tasks
    let started_task = started.notified();
    let _h1 = pool.submit(1).await.unwrap();
    let _h2 = pool.submit(2).await.unwrap();

    started_task.await;

    let stats = pool.stats();
    assert!(stats.running <= 4, "running should not exceed capacity");
    assert_eq!(stats.capacity, 4);
}

// ── 8. Concurrent submissions (10+ tasks) ────────────────────────────────────

#[tokio::test]
async fn concurrent_submissions_ten_plus() {
    let counter = Arc::new(AtomicU32::new(0));
    let pool = Pool::new(
        Arc::new(CountingHandler {
            counter: counter.clone(),
        }),
        PoolConfig::new("concurrent").with_size(4),
    );

    let mut handles = Vec::new();
    for i in 0..20u32 {
        handles.push(pool.submit(i).await.unwrap());
    }

    for h in handles {
        let result = h.result().await.unwrap();
        assert!(result < 20);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 20);
}

// ── 9. RoundRobinDispatcher ───────────────────────────────────────────────────

#[tokio::test]
async fn idle_pool_holds_no_permits() {
    let pool = Pool::new(
        Arc::new(IdleHandler),
        PoolConfig::new("idle-stats").with_size(3),
    );

    let stats = pool.stats();
    assert_eq!(stats.running, 0);
    assert_eq!(pool.available_permits(), 3);
}

#[tokio::test]
async fn dropping_pool_closes_runner() {
    let handler: Arc<dyn Handler<i32, i32>> = Arc::new(IdleHandler);
    let weak_handler = Arc::downgrade(&handler);
    let pool = Pool::new(handler, PoolConfig::new("drop-close").with_size(1));

    drop(pool);

    timeout(Duration::from_millis(200), async {
        loop {
            if weak_handler.upgrade().is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn round_robin_dispatcher_cycles() {
    let dispatcher = RoundRobinDispatcher::new(3);
    assert_eq!(dispatcher.next(), 0);
    assert_eq!(dispatcher.next(), 1);
    assert_eq!(dispatcher.next(), 2);
    assert_eq!(dispatcher.next(), 0); // wraps around
    assert_eq!(dispatcher.next(), 1);
}

#[tokio::test]
async fn round_robin_dispatcher_zero_slots() {
    let dispatcher = RoundRobinDispatcher::new(0);
    // Should not panic, returns 0
    assert_eq!(dispatcher.next(), 0);
}

// ── 10. Event factory functions ───────────────────────────────────────────────

#[tokio::test]
async fn event_factory_progress() {
    let id = uuid::Uuid::new_v4();
    let p = Progress::new(50, Some(100));
    let ev = Event::<i32>::progress(id, "w1", p);
    assert_eq!(ev.kind, EventKind::Progress);
    assert_eq!(ev.task_id, id);
    assert!(ev.progress.is_some());
    let prog = ev.progress.unwrap();
    assert_eq!(prog.current, 50);
    assert_eq!(prog.total, Some(100));
    assert!((prog.percent.unwrap() - 50.0).abs() < 0.01);
}

#[tokio::test]
async fn event_factory_partial() {
    let id = uuid::Uuid::new_v4();
    let ev = Event::partial(id, "w1", 42);
    assert_eq!(ev.kind, EventKind::Partial);
    assert_eq!(ev.data, Some(42));
}

#[tokio::test]
async fn event_factory_log() {
    let id = uuid::Uuid::new_v4();
    let ev = Event::<i32>::log(id, "w1", "hello");
    assert_eq!(ev.kind, EventKind::Log);
    assert_eq!(ev.error, Some("hello".to_string()));
}

#[tokio::test]
async fn event_factory_result() {
    let id = uuid::Uuid::new_v4();
    let ev = Event::result(id, "w1", 99);
    assert_eq!(ev.kind, EventKind::Result);
    assert_eq!(ev.data, Some(99));
}

#[tokio::test]
async fn event_factory_error() {
    let id = uuid::Uuid::new_v4();
    let ev = Event::<i32>::error(id, "w1", "boom");
    assert_eq!(ev.kind, EventKind::Error);
    assert_eq!(ev.error, Some("boom".to_string()));
}

// ── 11. EventKind variants all tested ─────────────────────────────────────────

#[tokio::test]
async fn event_kind_equality() {
    assert_eq!(EventKind::Progress, EventKind::Progress);
    assert_eq!(EventKind::Partial, EventKind::Partial);
    assert_eq!(EventKind::Log, EventKind::Log);
    assert_eq!(EventKind::Result, EventKind::Result);
    assert_eq!(EventKind::Error, EventKind::Error);
    assert_ne!(EventKind::Progress, EventKind::Error);
}

// ── 12. Progress computation ──────────────────────────────────────────────────

#[tokio::test]
async fn progress_percent_computation() {
    let p = Progress::new(75, Some(100));
    assert!((p.percent.unwrap() - 75.0).abs() < 0.01);

    let p_none = Progress::new(50, None);
    assert!(p_none.percent.is_none());

    let p_zero = Progress::new(0, Some(0));
    // total=0 → 100%
    assert!((p_zero.percent.unwrap() - 100.0).abs() < 0.01);
}

#[tokio::test]
async fn progress_with_message() {
    let p = Progress::new(1, Some(10)).with_message("loading");
    assert_eq!(p.message, Some("loading".to_string()));
}

// ── 13. Pool with single worker (serial execution) ───────────────────────────

#[tokio::test]
async fn single_worker_serial_execution() {
    let counter = Arc::new(AtomicU32::new(0));
    let pool = Pool::new(
        Arc::new(CountingHandler {
            counter: counter.clone(),
        }),
        PoolConfig::new("serial").with_size(1),
    );

    let mut handles = Vec::new();
    for i in 0..5u32 {
        handles.push(pool.submit(i).await.unwrap());
    }

    for h in handles {
        h.result().await.unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 5);
}

// ── 14. Provider bridge: from_provider roundtrip ──────────────────────────────

#[tokio::test]
async fn from_provider_roundtrip() {
    let provider = Arc::new(rskit_provider::request_response_fn(
        "add-one",
        |x: i32| async move { Ok(x + 1) },
    ));
    let handler = rskit_worker::from_provider(provider);
    let pool = Pool::new(Arc::new(handler), PoolConfig::new("fp-roundtrip"));

    let h = pool.submit(10).await.unwrap();
    assert_eq!(h.result().await.unwrap(), 11);
}

// ── 15. Provider bridge: as_provider roundtrip ────────────────────────────────

#[tokio::test]
async fn as_provider_roundtrip() {
    use rskit_provider::traits::RequestResponse;
    let handler: Arc<dyn Handler<i32, i32>> = Arc::new(DoubleHandler);
    let provider = rskit_worker::as_provider("double", handler);
    let result = provider.execute(7).await.unwrap();
    assert_eq!(result, 14);
}

// ── 16. Task IDs are unique ───────────────────────────────────────────────────

#[tokio::test]
async fn task_ids_are_unique() {
    let pool = Pool::new(
        Arc::new(DoubleHandler),
        PoolConfig::new("unique-ids").with_size(4),
    );

    let mut ids = std::collections::HashSet::new();
    let mut handles = Vec::new();

    for i in 0..10 {
        let h = pool.submit(i).await.unwrap();
        ids.insert(h.id);
        handles.push(h);
    }

    assert_eq!(ids.len(), 10, "all task IDs should be unique");

    for h in handles {
        h.result().await.unwrap();
    }
}

// ── 17. Default PoolConfig values ─────────────────────────────────────────────

#[tokio::test]
async fn default_pool_config_values() {
    let config = PoolConfig::default();
    assert_eq!(config.name, "pool");
    assert_eq!(config.queue_size, 256);
    assert_eq!(config.event_buffer, 64);
    assert_eq!(config.grace_period, Duration::from_secs(30));
    assert!(config.size > 0, "default size should be positive");
}

// ── 18. Multiple events from handler are all received ─────────────────────────

#[tokio::test]
async fn multiple_handler_events_received() {
    let pool = Pool::new(
        Arc::new(EventEmittingHandler),
        PoolConfig::new("multi-events").with_size(1),
    );

    let handle = pool.submit(10).await.unwrap();
    let mut rx = handle.events();

    let result = handle.result().await.unwrap();
    assert_eq!(result, 20);

    let mut kinds = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        kinds.push(ev.kind.clone());
    }

    // Handler emits: Progress, Partial, Log; then pool emits Result
    // We should see at least the Result event
    assert!(
        kinds.contains(&EventKind::Result),
        "expected at least Result event, got: {:?}",
        kinds
    );
}

// ── 19. Workload config clamps pool limits ─────────────────────────────────────

#[test]
fn workload_config_clamps_zero_limits_for_pool_config() {
    let config = WorkloadConfig {
        max_concurrent: 0,
        queue_size: 0,
    };

    let pool_config = config.to_pool_config("scheduler");

    assert_eq!(pool_config.name, "scheduler");
    assert_eq!(pool_config.size, 1);
    assert_eq!(pool_config.queue_size, 1);
}

// ── 20. Workload specs preserve resources and labels ──────────────────────────

#[test]
fn workload_spec_builders_preserve_resources_and_labels() {
    let spec = WorkloadSpec::new("ingest")
        .with_resources(ResourceRequirements {
            cpu_units: 4,
            memory_mib: 1024,
        })
        .with_label("tier", "batch")
        .with_label("region", "eu");

    assert_eq!(spec.name, "ingest");
    assert_eq!(spec.resources.cpu_units, 4);
    assert_eq!(spec.resources.memory_mib, 1024);
    assert_eq!(spec.labels["tier"], "batch");
    assert_eq!(spec.labels["region"], "eu");
}

// ── 21. Worker scheduler exposes placement and backs typed pools ──────────────

#[tokio::test]
async fn worker_scheduler_plans_and_builds_bounded_pool() {
    let scheduler = WorkerScheduler::new(
        "release-workers",
        WorkloadConfig {
            max_concurrent: 2,
            queue_size: 8,
        },
    );

    let plan = scheduler
        .plan_batch(
            WorkloadBatch::new("release")
                .with_workload(WorkloadSpec::new("lint"))
                .with_workload(WorkloadSpec::new("test")),
        )
        .await
        .unwrap();

    assert_eq!(plan.batch, "release");
    assert_eq!(plan.decisions.len(), 2);
    assert_eq!(plan.decisions[0].workload, "lint");
    assert_eq!(plan.decisions[1].pool, "release-workers");
    assert!(plan.decisions[1].reason.contains("capacity=2"));
    assert!(plan.decisions[1].reason.contains("queue=8"));

    let pool = scheduler.pool(Arc::new(DoubleHandler));
    let stats = pool.stats();
    assert_eq!(stats.name, "release-workers");
    assert_eq!(stats.capacity, 2);

    let handle = pool.submit(11).await.unwrap();
    assert_eq!(handle.result().await.unwrap(), 22);
}

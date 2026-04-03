//! Integration tests for agent-demo.
//!
//! Tests real task execution through the rskit-worker pool,
//! dashboard formatting, cancel mechanism, and helper functions.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rskit_errors::AppResult;
use rskit_worker::{EventKind, Pool, PoolConfig};

use agent_demo::dashboard::{self, TaskStatus, TrackedTask};
use agent_demo::shell;
use agent_demo::tasks::{AgentHandler, AgentTask, TaskOutput};

fn fixture_dir() -> PathBuf {
    shell::fixture_dir()
}

fn make_tracked(id: usize, label: &str, status: TaskStatus) -> TrackedTask {
    TrackedTask {
        id,
        label: label.to_string(),
        status,
        started: Instant::now(),
        progress: None,
        message: String::new(),
        result: None,
    }
}

// ── Task execution tests ──────────────────────────────────────────────

#[tokio::test]
async fn analyze_task_completes_with_real_fixture() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::Analyze {
        path: fixture_dir().join("image/real-photo.jpg"),
    };
    let handle = pool.submit(task).await?;
    let result = handle.result().await?;

    assert!(result.summary.contains("image/jpeg"));
    assert!(result.summary.contains("KB"));
    assert!(!result.details.is_empty());

    pool.shutdown().await.ok();
    Ok(())
}

#[tokio::test]
async fn resize_task_completes_with_real_fixture() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::Resize {
        path: fixture_dir().join("image/sample.png"),
        width: 100,
        height: 100,
    };
    let handle = pool.submit(task).await?;
    let result = handle.result().await?;

    assert!(result.summary.contains("100×100"));
    assert!(!result.details.is_empty());

    pool.shutdown().await.ok();
    Ok(())
}

#[tokio::test]
async fn pipeline_task_completes_with_real_fixture() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::Pipeline {
        path: fixture_dir().join("image/ai-generated.jpg"),
    };
    let handle = pool.submit(task).await?;
    let result = handle.result().await?;

    assert!(result.summary.contains("Pipeline complete"));
    assert!(result.summary.contains("crop"));

    pool.shutdown().await.ok();
    Ok(())
}

#[tokio::test]
async fn batch_task_processes_all_items() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::BatchProcess { count: 5 };
    let handle = pool.submit(task).await?;
    let result = handle.result().await?;

    assert_eq!(result.summary, "Processed 5 items");
    assert!(result.details.iter().any(|(k, v)| k == "Items" && v == "5"));

    pool.shutdown().await.ok();
    Ok(())
}

#[tokio::test]
async fn code_review_task_completes() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::CodeReview {
        path: fixture_dir().join("image/real-photo.jpg"),
    };
    let handle = pool.submit(task).await?;
    let result = handle.result().await?;

    assert!(result.summary.contains("Review complete"));
    assert!(result.details.iter().any(|(k, _)| k == "Complexity"));

    pool.shutdown().await.ok();
    Ok(())
}

// ── Parallel execution test ───────────────────────────────────────────

#[tokio::test]
async fn four_tasks_run_in_parallel() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(4),
    );

    let start = Instant::now();

    // Submit 4 batch tasks of 5 items each (~2s each)
    let mut task_handles = Vec::new();
    for _ in 0..4 {
        let handle = pool.submit(AgentTask::BatchProcess { count: 5 }).await?;
        task_handles.push(handle);
    }

    // Give workers a moment to pick up tasks
    tokio::time::sleep(Duration::from_millis(200)).await;

    // At least some should be running in parallel
    let stats = pool.stats();
    assert!(
        stats.running >= 2,
        "expected parallel execution, got {} running",
        stats.running
    );

    // Wait for all
    for h in task_handles {
        let result = h.result().await?;
        assert!(result.summary.contains("5 items"));
    }

    let elapsed = start.elapsed();
    // With 4 workers, 4 tasks should complete in roughly the time of 1 task, not 4x
    // Each 5-item batch takes ~1.5-2.5s, so all 4 in parallel should take < 5s
    assert!(
        elapsed < Duration::from_secs(8),
        "4 parallel tasks took {elapsed:?}, expected < 8s (sequential would be ~10s)"
    );

    pool.shutdown().await.ok();
    Ok(())
}

// ── Cancel test ───────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_stops_task() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    // Submit a long batch (100 items, ~40s normally)
    let task = AgentTask::BatchProcess { count: 100 };
    let handle = pool.submit(task).await?;

    // Wait a bit then cancel
    tokio::time::sleep(Duration::from_secs(1)).await;
    handle.cancel();

    // Result should be an error about cancellation
    let result = handle.result().await;
    assert!(result.is_err(), "cancelled task should return error");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("cancelled") || err.message.contains("cancel"),
        "error should mention cancel, got: {}",
        err.message
    );

    pool.shutdown().await.ok();
    Ok(())
}

// ── Progress events test ──────────────────────────────────────────────

#[tokio::test]
async fn tasks_emit_progress_events() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("test-pool").with_size(1),
    );

    let task = AgentTask::BatchProcess { count: 5 };
    let handle = pool.submit(task).await?;
    let mut events = handle.events();

    // Collect some progress events
    let mut progress_count = 0;
    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Ok(event) = events.recv() => {
                if event.kind == EventKind::Progress {
                    progress_count += 1;
                    assert!(event.progress.is_some());
                }
            }
            _ = &mut timeout => break,
        }
        // Got enough progress events
        if progress_count >= 5 {
            break;
        }
    }

    assert!(
        progress_count >= 5,
        "expected at least 5 progress events, got {progress_count}"
    );

    // Clean up
    let _ = handle.result().await;
    pool.shutdown().await.ok();
    Ok(())
}

// ── Dashboard formatting tests ────────────────────────────────────────

#[test]
fn status_table_renders_all_states() {
    let tasks = vec![
        make_tracked(1, "Analyze photo.jpg", TaskStatus::Running),
        {
            let mut t = make_tracked(2, "Resize img.png", TaskStatus::Done);
            t.result = Some(TaskOutput {
                summary: "200×200".into(),
                details: vec![],
            });
            t
        },
        {
            let mut t = make_tracked(3, "Pipeline bad.jpg", TaskStatus::Failed);
            t.result = Some(TaskOutput {
                summary: "error".into(),
                details: vec![],
            });
            t
        },
        make_tracked(4, "Review main.rs", TaskStatus::Cancelled),
    ];

    let table = dashboard::render_status_table(&tasks);
    assert!(table.contains("Analyze photo.jpg"));
    assert!(table.contains("Running"));
    assert!(table.contains("Done"));
    assert!(table.contains("Failed"));
    assert!(table.contains("Cancelled"));
    assert!(table.contains("100%")); // done task shows 100%
}

#[test]
fn log_result_formats_each_status() {
    let done = {
        let mut t = make_tracked(1, "X", TaskStatus::Done);
        t.result = Some(TaskOutput {
            summary: "ok".into(),
            details: vec![],
        });
        t
    };
    let result = dashboard::log_result(&done);
    assert!(result.contains("Completed"));

    let mut failed = make_tracked(2, "Y", TaskStatus::Failed);
    failed.result = Some(TaskOutput {
        summary: "err".into(),
        details: vec![],
    });
    assert!(dashboard::log_result(&failed).contains("Failed"));

    let cancelled = make_tracked(3, "Z", TaskStatus::Cancelled);
    assert!(dashboard::log_result(&cancelled).contains("Cancelled"));

    let running = make_tracked(4, "W", TaskStatus::Running);
    assert!(dashboard::log_result(&running).is_empty());
}

#[test]
fn status_bar_shows_pool_info() {
    let tasks = vec![
        make_tracked(1, "A", TaskStatus::Running),
        make_tracked(2, "B", TaskStatus::Running),
    ];
    let bar = dashboard::format_status_bar(&tasks, 2, 4, Duration::from_secs(10));
    assert!(bar.contains("2 running"));
    assert!(bar.contains("pool: 2/4"));
    assert!(bar.contains("10.0s"));
}

// ── Helper function tests ─────────────────────────────────────────────

#[test]
fn parse_dimensions_works() {
    assert_eq!(shell::parse_dimensions("200x150"), Some((200, 150)));
    assert_eq!(shell::parse_dimensions("bad"), None);
}

#[test]
fn fixture_dir_has_images() {
    let dir = shell::fixture_dir();
    assert!(dir.join("image/real-photo.jpg").exists());
    assert!(dir.join("image/sample.png").exists());
    assert!(dir.join("image/ai-generated.jpg").exists());
}

#[test]
fn resolve_path_finds_fixtures() {
    let p = shell::resolve_path("image/real-photo.jpg");
    assert!(p.exists(), "fixture should exist at {p:?}");
}

// ── Full demo scenario: all 4 task types in parallel ──────────────────

#[tokio::test]
async fn demo_tasks_all_complete_in_parallel() -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("demo-test").with_size(4),
    );

    let start = Instant::now();
    let fdir = fixture_dir();

    let h1 = pool
        .submit(AgentTask::Analyze {
            path: fdir.join("image/real-photo.jpg"),
        })
        .await?;
    let h2 = pool
        .submit(AgentTask::Resize {
            path: fdir.join("image/sample.png"),
            width: 64,
            height: 64,
        })
        .await?;
    let h3 = pool
        .submit(AgentTask::Pipeline {
            path: fdir.join("image/ai-generated.jpg"),
        })
        .await?;
    let h4 = pool
        .submit(AgentTask::CodeReview {
            path: fdir.join("image/real-photo.jpg"),
        })
        .await?;

    let r1 = h1.result().await?;
    let r2 = h2.result().await?;
    let r3 = h3.result().await?;
    let r4 = h4.result().await?;

    let elapsed = start.elapsed();

    // Verify each task produced correct output
    assert!(r1.summary.contains("image/jpeg"), "analyze: {}", r1.summary);
    assert!(r2.summary.contains("64×64"), "resize: {}", r2.summary);
    assert!(r3.summary.contains("Pipeline"), "pipeline: {}", r3.summary);
    assert!(r4.summary.contains("Review"), "review: {}", r4.summary);

    // The longest single task is CodeReview (~20s). If run sequentially, total would be ~57s.
    // In parallel (4 workers), should complete within the longest single task + overhead.
    assert!(
        elapsed < Duration::from_secs(30),
        "4 demo tasks in parallel took {elapsed:?}, expected < 30s (sequential would be ~57s)"
    );

    pool.shutdown().await.ok();
    Ok(())
}

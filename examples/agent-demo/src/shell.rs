//! Interactive command shell — dispatches tasks, shows progress, handles Ctrl+C.
//!
//! Key UX features:
//! - Spinners at the bottom show running tasks with step messages
//! - Completions print above active spinners via MultiProgress
//! - Async input: completions appear immediately, not after pressing Enter
//! - `/` prefix shows command menu (like Copilot's slash commands)
//! - `cancel <id>` to cancel running tasks

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rskit_cli::MultiProgress;
use rskit_errors::AppResult;
use rskit_worker::{EventKind, Pool, PoolConfig};

use crate::dashboard::{self, TaskStatus, TrackedTask};
use crate::tasks::{AgentHandler, AgentTask, TaskOutput};

/// Completion notification sent from background task listeners.
struct Completion {
    id: usize,
    result: Result<TaskOutput, String>,
}

/// Progress update from a running task.
struct ProgressUpdate {
    id: usize,
    current: u64,
    total: u64,
    message: String,
}

/// Handle to a running task's cancel token.
struct RunningHandle {
    id: usize,
    cancel: tokio_util::sync::CancellationToken,
}

pub async fn run(cancel: tokio_util::sync::CancellationToken) -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("agent-pool").with_size(4),
    );

    let multi = MultiProgress::new();
    let mut tasks: Vec<TrackedTask> = Vec::new();
    let mut handles: Vec<RunningHandle> = Vec::new();
    let mut next_id: usize = 1;

    // Channel for background task completions
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<Completion>();
    // Channel for progress updates (keeps TrackedTask in sync)
    let (prog_tx, mut prog_rx) = tokio::sync::mpsc::unbounded_channel::<ProgressUpdate>();

    // Channel for stdin lines (async reading)
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(1);
    tokio::task::spawn_blocking(move || {
        loop {
            let mut buf = String::new();
            if io::stdin().lock().read_line(&mut buf).is_err() {
                break;
            }
            let trimmed = buf.trim().to_string();
            if line_tx.blocking_send(trimmed).is_err() {
                break;
            }
        }
    });

    // Print initial banner via multi so it appears above any bars
    multi.println(BANNER).ok();
    multi.println(&format_help()).ok();
    print_prompt();

    loop {
        tokio::select! {
            // Handle user input
            Some(line) = line_rx.recv() => {
                if line.is_empty() {
                    print_prompt();
                    continue;
                }

                // Strip optional / prefix
                let cmd_line = line.strip_prefix('/').unwrap_or(&line);
                let parts: Vec<&str> = cmd_line.splitn(3, ' ').collect();

                match parts[0] {
                    "help" | "h" | "?" | "" => {
                        multi.println(&format_help()).ok();
                    }

                    "analyze" | "a" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(&format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            let task = AgentTask::Analyze { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "resize" | "r" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(&format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            let (w, h) = if parts.len() >= 3 {
                                parse_dimensions(parts[2]).unwrap_or((200, 200))
                            } else {
                                (200, 200)
                            };
                            let task = AgentTask::Resize { path, width: w, height: h };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "pipeline" | "p" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(&format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            let task = AgentTask::Pipeline { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "batch" | "b" => {
                        let count = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
                        let task = AgentTask::BatchProcess { count };
                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                    }

                    "review" | "rv" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(&format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            let task = AgentTask::CodeReview { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "demo" | "d" => {
                        multi.println("  \x1b[1;33m⚡ Launching demo\x1b[0m — 4 parallel agents on real fixtures\n").ok();
                        let fixtures = fixture_dir();

                        let t1 = AgentTask::Analyze {
                            path: fixtures.join("image/real-photo.jpg"),
                        };
                        let t2 = AgentTask::Resize {
                            path: fixtures.join("image/sample.png"),
                            width: 150,
                            height: 150,
                        };
                        let t3 = AgentTask::Pipeline {
                            path: fixtures.join("image/ai-generated.jpg"),
                        };
                        let t4 = AgentTask::CodeReview {
                            path: fixtures.join("image/real-photo.jpg"),
                        };

                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, t1).await?;
                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, t2).await?;
                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, t3).await?;
                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, t4).await?;
                    }

                    "cancel" if parts.len() >= 2 => {
                        if let Ok(id) = parts[1].parse::<usize>() {
                            if let Some(h) = handles.iter().find(|h| h.id == id) {
                                h.cancel.cancel();
                                if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
                                    t.status = TaskStatus::Cancelled;
                                    t.message = "Cancelled by user".into();
                                    let msg = dashboard::render_completion(t);
                                    if !msg.is_empty() {
                                        multi.println(&msg).ok();
                                    }
                                }
                            } else {
                                multi.println(&format!("  \x1b[31m✗ No running task with ID {id}\x1b[0m")).ok();
                            }
                        }
                    }

                    "status" | "s" => {
                        drain_completions(&mut done_rx, &mut tasks, &multi);
                        let table = dashboard::render_status_table(&tasks);
                        multi.println(&table).ok();
                    }

                    "detail" if parts.len() >= 2 => {
                        if let Ok(id) = parts[1].parse::<usize>() {
                            if let Some(t) = tasks.iter().find(|t| t.id == id) {
                                let details = dashboard::render_task_details(t);
                                multi.println(&details).ok();
                            } else {
                                multi.println(&format!("  \x1b[31m✗ Unknown task ID:\x1b[0m {id}")).ok();
                            }
                        }
                    }

                    "stats" => {
                        let st = pool.stats();
                        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
                        let done = tasks.iter().filter(|t| t.status == TaskStatus::Done).count();
                        let failed = tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
                        let cancelled = tasks.iter().filter(|t| t.status == TaskStatus::Cancelled).count();
                        let msg = format!(
                            "\n  \x1b[1mWorker Pool\x1b[0m: {} active / {} capacity\n  \x1b[1mTasks\x1b[0m: {} running, {} done, {} failed, {} cancelled, {} total\n",
                            st.running, st.capacity, running, done, failed, cancelled, tasks.len()
                        );
                        multi.println(&msg).ok();
                    }

                    "clear" | "c" => {
                        let before = tasks.len();
                        tasks.retain(|t| t.status == TaskStatus::Running);
                        handles.retain(|h| tasks.iter().any(|t| t.id == h.id));
                        multi.println(&format!(
                            "  \x1b[2mCleared {} completed task(s).\x1b[0m",
                            before - tasks.len()
                        )).ok();
                    }

                    "quit" | "q" | "exit" => {
                        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
                        if running > 0 {
                            multi.println(&format!(
                                "  \x1b[33mShutting down... cancelling {} running task(s)\x1b[0m",
                                running
                            )).ok();
                            for h in &handles {
                                h.cancel.cancel();
                            }
                        }
                        break;
                    }

                    other => {
                        multi.println(&format!(
                            "  \x1b[31m✗ Unknown command:\x1b[0m {other}\n  Type \x1b[1m/help\x1b[0m or \x1b[1m?\x1b[0m for commands."
                        )).ok();
                    }
                }

                print_prompt();
            }

            // Handle background task completions (appear immediately!)
            Some(comp) = done_rx.recv() => {
                if let Some(t) = tasks.iter_mut().find(|t| t.id == comp.id) {
                    // Don't overwrite Cancelled status
                    if t.status == TaskStatus::Running {
                        match comp.result {
                            Ok(output) => {
                                t.status = TaskStatus::Done;
                                t.result = Some(output);
                            }
                            Err(err) => {
                                if err.contains("cancelled") {
                                    t.status = TaskStatus::Cancelled;
                                } else {
                                    t.status = TaskStatus::Failed;
                                }
                                t.result = Some(TaskOutput {
                                    summary: err,
                                    details: vec![],
                                });
                            }
                        }
                        let msg = dashboard::render_completion(t);
                        if !msg.is_empty() {
                            multi.println(&msg).ok();
                        }
                    }
                }
                handles.retain(|h| h.id != comp.id);
            }

            // Handle progress updates from running tasks
            Some(update) = prog_rx.recv() => {
                if let Some(t) = tasks.iter_mut().find(|t| t.id == update.id) {
                    t.progress = Some((update.current, update.total));
                    t.message = update.message;
                }
            }

            // Ctrl+C
            _ = cancel.cancelled() => {
                multi.println("  \x1b[33m⚠ Interrupted\x1b[0m").ok();
                for h in &handles { h.cancel.cancel(); }
                break;
            }
        }
    }

    // Graceful shutdown
    let running: Vec<_> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .collect();
    if !running.is_empty() {
        multi
            .println(&format!(
                "  \x1b[2mWaiting for {} task(s) to finish...\x1b[0m",
                running.len()
            ))
            .ok();
    }
    pool.shutdown().await.ok();
    multi.println("  \x1b[1;32m👋 Done.\x1b[0m\n").ok();
    Ok(())
}

/// Submit a task to the pool and attach a spinner + event listener.
async fn submit_task(
    pool: &Pool<AgentTask, TaskOutput>,
    multi: &MultiProgress,
    tasks: &mut Vec<TrackedTask>,
    handles: &mut Vec<RunningHandle>,
    done_tx: &tokio::sync::mpsc::UnboundedSender<Completion>,
    prog_tx: &tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
    next_id: &mut usize,
    task: AgentTask,
) -> AppResult<()> {
    let id = *next_id;
    *next_id += 1;
    let label = task.to_string();

    // Use a spinner — it shows "{spinner} {prefix} {wide_msg}" with animation
    let spinner = multi.add_spinner(&format!("\x1b[33mTask #{id}\x1b[0m"));
    spinner.set_message(format!("({label}) — \x1b[2mSpawning...\x1b[0m"));

    let handle = pool.submit(task).await?;

    // Clone the pool's cancel token BEFORE consuming the handle — this is the
    // same token the handler receives, so cancelling it actually stops work.
    let task_cancel = handle.cancel_token();
    handles.push(RunningHandle {
        id,
        cancel: task_cancel,
    });

    multi.println(&format!(
        "  \x1b[33m● Agent #{id}\x1b[0m ({label}) — \x1b[33mSpawned\x1b[0m"
    )).ok();

    tasks.push(TrackedTask {
        id,
        label: label.clone(),
        status: TaskStatus::Running,
        started: Instant::now(),
        progress: None,
        message: "Starting...".into(),
        result: None,
    });

    // Spawn listener for events and result
    let done_tx = done_tx.clone();
    let prog_tx = prog_tx.clone();
    let spinner_inner = spinner.inner().clone();
    let label_clone = label;
    tokio::spawn(async move {
        let mut events = handle.events();

        // Listen for progress events — update spinner message AND tracked progress
        let event_spinner = spinner_inner.clone();
        let event_label = label_clone.clone();
        let event_prog_tx = prog_tx;
        let event_loop = tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if event.kind == EventKind::Progress {
                    if let Some(ref p) = event.progress {
                        let pct = p.percent.unwrap_or(0.0) as u32;
                        let step_msg = p
                            .message
                            .as_deref()
                            .unwrap_or("Working...");
                        event_spinner.set_message(format!(
                            "({event_label}) — \x1b[2m{step_msg}\x1b[0m [{pct}%]"
                        ));
                        // Also update the TrackedTask so /status shows progress
                        let current = p.current;
                        let total = p.total.unwrap_or(0);
                        let _ = event_prog_tx.send(ProgressUpdate {
                            id,
                            current,
                            total,
                            message: step_msg.to_string(),
                        });
                    }
                }
            }
        });

        // Wait for the final result
        let result = match handle.result().await {
            Ok(output) => Ok(output),
            Err(e) => Err(e.message),
        };

        event_loop.abort();
        spinner_inner.finish_and_clear();
        let _ = done_tx.send(Completion { id, result });
    });

    Ok(())
}

/// Drain pending completions from the channel (non-blocking).
fn drain_completions(
    done_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Completion>,
    tasks: &mut Vec<TrackedTask>,
    multi: &MultiProgress,
) {
    while let Ok(comp) = done_rx.try_recv() {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == comp.id) {
            if t.status == TaskStatus::Running {
                match comp.result {
                    Ok(output) => {
                        t.status = TaskStatus::Done;
                        t.result = Some(output);
                    }
                    Err(err) => {
                        if err.contains("cancelled") {
                            t.status = TaskStatus::Cancelled;
                        } else {
                            t.status = TaskStatus::Failed;
                        }
                        t.result = Some(TaskOutput {
                            summary: err,
                            details: vec![],
                        });
                    }
                }
                let msg = dashboard::render_completion(t);
                if !msg.is_empty() {
                    multi.println(&msg).ok();
                }
            }
        }
    }
}

fn print_prompt() {
    print!("\x1b[1;36m❯\x1b[0m ");
    io::stdout().flush().ok();
}

fn format_help() -> String {
    let mut s = String::new();
    s.push_str("\n  \x1b[1;36m┌─────────────────────────────────────────────┐\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[1mAgent Commands\x1b[0m                             \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/demo\x1b[0m              Launch 4 parallel agents \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/analyze\x1b[0m <file>     Detect MIME & metadata  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/resize\x1b[0m  <file>     Resize image (200×200)  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/pipeline\x1b[0m <file>    Resize → crop → rotate  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/review\x1b[0m  <file>     Code review simulation  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/batch\x1b[0m   [count]    Batch processing (×20)  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/status\x1b[0m             Show all tasks          \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/detail\x1b[0m  <id>       Task details            \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/cancel\x1b[0m  <id>       Cancel running task     \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/stats\x1b[0m              Worker pool stats       \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/clear\x1b[0m              Clear completed tasks   \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/quit\x1b[0m               Exit                    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m└─────────────────────────────────────────────┘\x1b[0m\n");
    s
}

fn resolve_path(input: &str) -> PathBuf {
    let p = PathBuf::from(input);
    if p.is_absolute() {
        p
    } else if p.exists() {
        p
    } else {
        fixture_dir().join(input)
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

fn parse_dimensions(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        Some((w, h))
    } else {
        None
    }
}

const BANNER: &str = concat!(
    "\n",
    "  \x1b[1;36m🚀 rskit Agent Demo\x1b[0m — Media Processing Pipeline\n",
    "  \x1b[2mShowcasing background workers, progress tracking, and stream processing\x1b[0m\n",
    "  \x1b[2mrskit-worker │ rskit-cli │ rskit-pipeline │ rskit-file │ rskit-media-image\x1b[0m\n",
    "  \x1b[2mType /help or ? for commands. Prefix commands with / for menu style.\x1b[0m\n",
);

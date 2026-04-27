//! Interactive command shell with Copilot-inspired UI.
//!
//! Terminal layout (bottom of visible area):
//!   [activity log — scrolls up via multi.println()]
//!   ⠋ Agent #1 (Analyze …) — step message [37%]     ← task spinners
//!   ⠋ Agent #2 (Resize …)  — step message [50%]
//!   ─── 2 running · 1 done │ pool: 2/4 │ 12.5s      ← persistent status bar
//!   ❯ [user input]                                    ← prompt

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rskit_errors::AppResult;
use rskit_worker::{EventKind, Pool, PoolConfig};

use crate::dashboard::{self, TaskStatus, TrackedTask};
use crate::tasks::{AgentHandler, AgentTask, TaskOutput};

// ── Channel message types ─────────────────────────────────────────────

struct Completion {
    id: usize,
    result: Result<TaskOutput, String>,
}

struct ProgressUpdate {
    id: usize,
    current: u64,
    total: u64,
    message: String,
}

struct RunningHandle {
    id: usize,
    cancel: tokio_util::sync::CancellationToken,
    spinner: ProgressBar,
}

const POOL_SIZE: usize = 4;

// ── Main loop ─────────────────────────────────────────────────────────

pub async fn run(cancel: tokio_util::sync::CancellationToken) -> AppResult<()> {
    let pool = Pool::new(
        Arc::new(AgentHandler),
        PoolConfig::new("agent-pool").with_size(POOL_SIZE),
    );

    let multi = MultiProgress::new();
    let mut tasks: Vec<TrackedTask> = Vec::new();
    let mut handles: Vec<RunningHandle> = Vec::new();
    let mut next_id: usize = 1;
    let start_time = Instant::now();

    // Channels
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<Completion>();
    let (prog_tx, mut prog_rx) = tokio::sync::mpsc::unbounded_channel::<ProgressUpdate>();

    // Async stdin reader
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(1);
    let cancel_for_stdin = cancel.clone();
    tokio::task::spawn_blocking(move || {
        let stdin = io::stdin();
        loop {
            if cancel_for_stdin.is_cancelled() {
                break;
            }
            let mut buf = String::new();
            match stdin.lock().read_line(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = buf.trim().to_string();
                    if line_tx.blocking_send(trimmed).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Banner + instructions
    println!("{BANNER}");
    println!(
        "  \x1b[2mType\x1b[0m \x1b[1m/help\x1b[0m \x1b[2mor\x1b[0m \x1b[1m?\x1b[0m \x1b[2mfor commands. Prefix with\x1b[0m \x1b[1m/\x1b[0m \x1b[2mfor menu style.\x1b[0m"
    );
    print_status_line(&tasks, &pool, start_time);
    print_prompt();

    loop {
        tokio::select! {
            // ── User input ────────────────────────────────────
            Some(line) = line_rx.recv() => {
                if line.is_empty() {
                    print_prompt();
                    continue;
                }

                let cmd_line = line.strip_prefix('/').unwrap_or(&line);
                let parts: Vec<&str> = cmd_line.splitn(3, ' ').collect();

                match parts[0] {
                    "help" | "h" | "?" | "" => {
                        multi.println(format_help()).ok();
                    }

                    "analyze" | "a" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            multi.println(dashboard::log_thinking(
                                &format!("Analyzing {} for MIME type, metadata, and structural properties.", parts[1])
                            )).ok();
                            let task = AgentTask::Analyze { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "resize" | "r" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            let (w, h) = if parts.len() >= 3 {
                                parse_dimensions(parts[2]).unwrap_or((200, 200))
                            } else {
                                (200, 200)
                            };
                            multi.println(dashboard::log_thinking(
                                &format!("Resizing {} to {}×{} using Fit mode.", parts[1], w, h)
                            )).ok();
                            let task = AgentTask::Resize { path, width: w, height: h };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "pipeline" | "p" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            multi.println(dashboard::log_thinking(
                                &format!("Running 3-step pipeline on {}: resize → crop → rotate.", parts[1])
                            )).ok();
                            let task = AgentTask::Pipeline { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "batch" | "b" => {
                        let count = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
                        multi.println(dashboard::log_thinking(
                            &format!("Starting batch processing of {count} items.")
                        )).ok();
                        let task = AgentTask::BatchProcess { count };
                        submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                    }

                    "review" | "rv" if parts.len() >= 2 => {
                        let path = resolve_path(parts[1]);
                        if !path.exists() {
                            multi.println(format!(
                                "  \x1b[31m✗ File not found:\x1b[0m {}",
                                path.display()
                            )).ok();
                        } else {
                            multi.println(dashboard::log_thinking(
                                &format!("Running code review on {}: AST analysis, security, complexity.", parts[1])
                            )).ok();
                            let task = AgentTask::CodeReview { path };
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                    }

                    "demo" | "d" => {
                        multi.println("").ok();
                        multi.println(dashboard::log_thinking(
                            "Spawning 4 parallel agents to process real media fixtures."
                        )).ok();
                        multi.println(dashboard::log_thinking(
                            "Each agent runs independently — watch spinners and completions."
                        )).ok();
                        multi.println("").ok();

                        let fix = fixture_dir();
                        let demo_tasks = vec![
                            AgentTask::Analyze {
                                path: fix.join("image/real-photo.jpg"),
                            },
                            AgentTask::Resize {
                                path: fix.join("image/sample.png"),
                                width: 150,
                                height: 150,
                            },
                            AgentTask::Pipeline {
                                path: fix.join("image/ai-generated.jpg"),
                            },
                            AgentTask::CodeReview {
                                path: fix.join("image/real-photo.jpg"),
                            },
                        ];
                        for task in demo_tasks {
                            submit_task(&pool, &multi, &mut tasks, &mut handles, &done_tx, &prog_tx, &mut next_id, task).await?;
                        }
                        multi.println("").ok();
                    }

                    "cancel" if parts.len() >= 2 => {
                        if let Ok(id) = parts[1].parse::<usize>() {
                            if let Some(h) = handles.iter().find(|h| h.id == id) {
                                h.cancel.cancel();
                                if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
                                    t.status = TaskStatus::Cancelled;
                                    t.message = "Cancelled by user".into();
                                    multi.println(dashboard::log_cancel(t)).ok();
                                }
                            } else {
                                multi.println(format!(
                                    "  \x1b[31m✗ No running task with ID {id}\x1b[0m"
                                )).ok();
                            }
                        }
                    }

                    "status" | "s" => {
                        drain_completions(&mut done_rx, &mut tasks, &mut handles, &multi);
                        multi.println(dashboard::render_status_table(&tasks)).ok();
                    }

                    "detail" if parts.len() >= 2 => {
                        if let Ok(id) = parts[1].parse::<usize>() {
                            if let Some(t) = tasks.iter().find(|t| t.id == id) {
                                multi.println(dashboard::render_task_details(t)).ok();
                            } else {
                                multi.println(format!(
                                    "  \x1b[31m✗ Unknown task ID:\x1b[0m {id}"
                                )).ok();
                            }
                        }
                    }

                    "stats" => {
                        let st = pool.stats();
                        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
                        let done = tasks.iter().filter(|t| t.status == TaskStatus::Done).count();
                        let failed = tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
                        let cancelled = tasks.iter().filter(|t| t.status == TaskStatus::Cancelled).count();
                        multi.println(format!(
                            "\n  \x1b[1mWorker Pool\x1b[0m: {} active / {} capacity\n  \x1b[1mTasks\x1b[0m: {} running, {} done, {} failed, {} cancelled, {} total\n  \x1b[1mUptime\x1b[0m: {}\n",
                            st.running, st.capacity, running, done, failed, cancelled, tasks.len(),
                            dashboard::format_duration(start_time.elapsed())
                        )).ok();
                    }

                    "clear" | "c" => {
                        let before = tasks.len();
                        tasks.retain(|t| t.status == TaskStatus::Running);
                        handles.retain(|h| tasks.iter().any(|t| t.id == h.id));
                        multi.println(format!(
                            "  \x1b[2mCleared {} completed task(s).\x1b[0m",
                            before - tasks.len()
                        )).ok();
                    }

                    "quit" | "q" | "exit" => {
                        let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
                        if running > 0 {
                            multi.println(format!(
                                "  \x1b[33m⚠ Shutting down — cancelling {running} running task(s)\x1b[0m"
                            )).ok();
                            for h in &handles {
                                h.cancel.cancel();
                            }
                        }
                        break;
                    }

                    other => {
                        multi.println(format!(
                            "  \x1b[31m✗ Unknown command:\x1b[0m {other}\n  Type \x1b[1m/help\x1b[0m for commands."
                        )).ok();
                    }
                }

                print_prompt();
            }

            // ── Task completion (appears immediately!) ────────
            Some(comp) = done_rx.recv() => {
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
                        multi.println(dashboard::log_result(t)).ok();
                    }
                }
                // Clean up spinner for completed task
                if let Some(pos) = handles.iter().position(|h| h.id == comp.id) {
                    handles[pos].spinner.finish_and_clear();
                    handles.remove(pos);
                }
            }

            // ── Progress updates ──────────────────────────────
            Some(update) = prog_rx.recv() => {
                if let Some(t) = tasks.iter_mut().find(|t| t.id == update.id) {
                    t.progress = Some((update.current, update.total));
                    t.message = update.message;
                }
            }

            // ── Ctrl+C ───────────────────────────────────────
            _ = cancel.cancelled() => {
                multi.println("  \x1b[33m⚠ Interrupted\x1b[0m").ok();
                for h in &handles {
                    h.cancel.cancel();
                }
                break;
            }
        }
    }

    // Graceful shutdown
    cancel.cancel();
    for h in &handles {
        h.spinner.finish_and_clear();
    }
    let running = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    if running > 0 {
        println!("  \x1b[2mWaiting for {running} task(s) to finish...\x1b[0m");
    }
    pool.shutdown().await.ok();
    println!("  \x1b[1;32m👋 Done.\x1b[0m");

    // Force exit — the blocking stdin thread can't be interrupted
    #[allow(clippy::disallowed_methods)]
    std::process::exit(0);
}

// ── Task submission ───────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
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

    // Add spinner to MultiProgress
    let spinner = multi.add(ProgressBar::new_spinner());
    spinner.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} \x1b[1mAgent #{prefix}\x1b[0m {wide_msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "]),
    );
    spinner.set_prefix(id.to_string());
    spinner.set_message(format!("({label}) \x1b[2m— Spawning...\x1b[0m"));
    spinner.enable_steady_tick(Duration::from_millis(80));

    let handle = pool.submit(task).await?;
    let task_cancel = handle.cancel_token();

    // Activity log: spawn notification
    multi.println(dashboard::log_spawn(id, &label)).ok();

    tasks.push(TrackedTask {
        id,
        label: label.clone(),
        status: TaskStatus::Running,
        started: Instant::now(),
        progress: None,
        message: "Starting...".into(),
        result: None,
    });

    // Spawn event listener
    let done_tx = done_tx.clone();
    let prog_tx = prog_tx.clone();
    let spinner_clone = spinner.clone();
    tokio::spawn(async move {
        let mut events = handle.events();

        // Listen for progress events → update spinner + tracked progress
        let event_spinner = spinner_clone.clone();
        let event_label = label.clone();
        let event_prog_tx = prog_tx;
        let event_loop = tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if event.kind == EventKind::Progress {
                            if let Some(ref p) = event.progress {
                                let pct = p.percent.unwrap_or(0.0) as u32;
                                let step_msg = p.message.as_deref().unwrap_or("Working...");
                                event_spinner.set_message(format!(
                                    "({event_label}) \x1b[2m— {step_msg}\x1b[0m \x1b[36m[{pct}%]\x1b[0m"
                                ));
                                let _ = event_prog_tx.send(ProgressUpdate {
                                    id,
                                    current: p.current,
                                    total: p.total.unwrap_or(0),
                                    message: step_msg.to_string(),
                                });
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });

        // Wait for final result
        let result = match handle.result().await {
            Ok(output) => Ok(output),
            Err(e) => Err(e.message),
        };

        event_loop.abort();
        // Spinner cleanup handled by completion handler in main loop
        let _ = done_tx.send(Completion { id, result });
    });

    handles.push(RunningHandle {
        id,
        cancel: task_cancel,
        spinner,
    });

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────

fn drain_completions(
    done_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Completion>,
    tasks: &mut [TrackedTask],
    handles: &mut Vec<RunningHandle>,
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
                multi.println(dashboard::log_result(t)).ok();
            }
        }
        if let Some(pos) = handles.iter().position(|h| h.id == comp.id) {
            handles[pos].spinner.finish_and_clear();
            handles.remove(pos);
        }
    }
}

fn print_prompt() {
    print!("\x1b[1;36m❯\x1b[0m ");
    io::stdout().flush().ok();
}

fn print_status_line(
    tasks: &[TrackedTask],
    pool: &Pool<AgentTask, TaskOutput>,
    start_time: Instant,
) {
    let st = pool.stats();
    let bar = dashboard::format_status_bar(tasks, st.running, st.capacity, start_time.elapsed());
    println!("  {bar}");
}

fn format_help() -> String {
    let mut s = String::new();
    s.push_str("\n  \x1b[1;36m┌─────────────────────────────────────────────────┐\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[1mAgent Commands\x1b[0m                                 \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/demo\x1b[0m                Launch 4 parallel agents  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/analyze\x1b[0m <file>      Detect MIME & metadata    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/resize\x1b[0m  <file> [WxH] Resize image             \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/pipeline\x1b[0m <file>     Resize → crop → rotate    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/review\x1b[0m  <file>      Code review simulation    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/batch\x1b[0m   [count]     Batch processing (×30)    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/status\x1b[0m              Show all tasks             \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/detail\x1b[0m  <id>        Task details               \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/cancel\x1b[0m  <id>        Cancel running task        \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/stats\x1b[0m               Worker pool stats          \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/clear\x1b[0m               Clear completed tasks      \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/quit\x1b[0m                Exit                       \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m└─────────────────────────────────────────────────┘\x1b[0m\n");
    s
}

pub fn resolve_path(input: &str) -> PathBuf {
    let p = PathBuf::from(input);
    if p.is_absolute() || p.exists() {
        p
    } else {
        fixture_dir().join(input)
    }
}

pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

pub fn parse_dimensions(s: &str) -> Option<(u32, u32)> {
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
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_uses_fixtures_for_relative() {
        let p = resolve_path("image/real-photo.jpg");
        assert!(
            p.to_string_lossy()
                .contains("tests/fixtures/image/real-photo.jpg")
        );
    }

    #[test]
    fn resolve_path_keeps_absolute() {
        let p = resolve_path("/tmp/test.jpg");
        assert_eq!(p, PathBuf::from("/tmp/test.jpg"));
    }

    #[test]
    fn parse_dimensions_valid() {
        assert_eq!(parse_dimensions("200x150"), Some((200, 150)));
        assert_eq!(parse_dimensions("1920x1080"), Some((1920, 1080)));
    }

    #[test]
    fn parse_dimensions_invalid() {
        assert_eq!(parse_dimensions("abc"), None);
        assert_eq!(parse_dimensions("200"), None);
        assert_eq!(parse_dimensions("200xabc"), None);
    }

    #[test]
    fn fixture_dir_exists() {
        let dir = fixture_dir();
        assert!(dir.exists(), "fixtures dir should exist at {dir:?}");
    }

    #[test]
    fn format_help_contains_all_commands() {
        let help = format_help();
        for cmd in &[
            "/demo",
            "/analyze",
            "/resize",
            "/pipeline",
            "/review",
            "/batch",
            "/status",
            "/detail",
            "/cancel",
            "/stats",
            "/clear",
            "/quit",
        ] {
            assert!(help.contains(cmd), "help should contain {cmd}");
        }
    }

    #[test]
    fn banner_is_not_empty() {
        assert!(BANNER.len() > 1);
        assert!(BANNER.contains("rskit"));
    }
}

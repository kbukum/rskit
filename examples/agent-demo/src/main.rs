//! Agent Demo — Interactive media processing agent built with rskit.
//!
//! Demonstrates the public `rskit` facade modules: `worker` background tasks,
//! `cli` output helpers, `storage` I/O, and `media` + `media_image` processing.
//!
//! Run: cargo run -p agent-demo

#[cfg(not(test))]
mod interactive {
    use std::io::{self, BufRead, Write};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
    use rskit::AppResult;
    use rskit::util::time::format_duration;
    use rskit::worker::{EventKind, Pool, PoolConfig};

    use agent_demo::dashboard::{self, TaskStatus, TrackedTask};
    use agent_demo::shell::{BANNER, fixture_dir, format_help, parse_dimensions, resolve_path};
    use agent_demo::tasks::{AgentHandler, AgentTask, TaskOutput};

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

    struct AppRuntime {
        pool: Pool<AgentTask, TaskOutput>,
        multi: MultiProgress,
        tasks: Vec<TrackedTask>,
        handles: Vec<RunningHandle>,
        next_id: usize,
        start_time: Instant,
        done_tx: tokio::sync::mpsc::UnboundedSender<Completion>,
        prog_tx: tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
    }

    impl AppRuntime {
        fn new(
            pool: Pool<AgentTask, TaskOutput>,
            multi: MultiProgress,
            done_tx: tokio::sync::mpsc::UnboundedSender<Completion>,
            prog_tx: tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
        ) -> Self {
            Self {
                pool,
                multi,
                tasks: Vec::new(),
                handles: Vec::new(),
                next_id: 1,
                start_time: Instant::now(),
                done_tx,
                prog_tx,
            }
        }

        fn print_intro(&self) {
            println!("{BANNER}");
            println!(
                "  \x1b[2mType\x1b[0m \x1b[1m/help\x1b[0m \x1b[2mor\x1b[0m \x1b[1m?\x1b[0m \x1b[2mfor commands. Prefix with\x1b[0m \x1b[1m/\x1b[0m \x1b[2mfor menu style.\x1b[0m"
            );
            self.print_status_line();
            print_prompt();
        }

        async fn handle_line(
            &mut self,
            line: &str,
            done_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Completion>,
        ) -> AppResult<bool> {
            if line.is_empty() {
                return Ok(false);
            }

            let cmd_line = line.strip_prefix('/').unwrap_or(line);
            let parts: Vec<&str> = cmd_line.splitn(3, ' ').collect();
            match parts[0] {
                "help" | "h" | "?" | "" => {
                    self.multi.println(format_help()).ok();
                }
                "analyze" | "a" if parts.len() >= 2 => self.analyze(parts[1]).await?,
                "resize" | "r" if parts.len() >= 2 => self.resize(&parts).await?,
                "pipeline" | "p" if parts.len() >= 2 => self.pipeline(parts[1]).await?,
                "batch" | "b" => self.batch(&parts).await?,
                "review" | "rv" if parts.len() >= 2 => self.review(parts[1]).await?,
                "demo" | "d" => self.demo().await?,
                "cancel" if parts.len() >= 2 => self.cancel_task(parts[1]),
                "status" | "s" => self.show_status(done_rx),
                "detail" if parts.len() >= 2 => self.show_detail(parts[1]),
                "stats" => self.show_stats(),
                "clear" | "c" => self.clear_completed(),
                "quit" | "q" | "exit" => return Ok(self.quit_requested()),
                other => self.unknown_command(other),
            }
            Ok(false)
        }

        async fn analyze(&mut self, path_arg: &str) -> AppResult<()> {
            let path = resolve_path(path_arg);
            if path.exists() {
                self.multi
                    .println(dashboard::log_thinking(&format!(
                        "Analyzing {path_arg} for MIME type, metadata, and structural properties."
                    )))
                    .ok();
                self.submit(AgentTask::Analyze { path }).await
            } else {
                self.print_missing_path(&path);
                Ok(())
            }
        }

        async fn resize(&mut self, parts: &[&str]) -> AppResult<()> {
            let path = resolve_path(parts[1]);
            if path.exists() {
                let (width, height) = if parts.len() >= 3 {
                    parse_dimensions(parts[2]).unwrap_or((200, 200))
                } else {
                    (200, 200)
                };
                self.multi
                    .println(dashboard::log_thinking(&format!(
                        "Resizing {} to {}×{} using Fit mode.",
                        parts[1], width, height
                    )))
                    .ok();
                self.submit(AgentTask::Resize {
                    path,
                    width,
                    height,
                })
                .await
            } else {
                self.print_missing_path(&path);
                Ok(())
            }
        }

        async fn pipeline(&mut self, path_arg: &str) -> AppResult<()> {
            let path = resolve_path(path_arg);
            if path.exists() {
                self.multi
                    .println(dashboard::log_thinking(&format!(
                        "Running 3-step pipeline on {path_arg}: resize → crop → rotate."
                    )))
                    .ok();
                self.submit(AgentTask::Pipeline { path }).await
            } else {
                self.print_missing_path(&path);
                Ok(())
            }
        }

        async fn batch(&mut self, parts: &[&str]) -> AppResult<()> {
            let count = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
            self.multi
                .println(dashboard::log_thinking(&format!(
                    "Starting batch processing of {count} items."
                )))
                .ok();
            self.submit(AgentTask::BatchProcess { count }).await
        }

        async fn review(&mut self, path_arg: &str) -> AppResult<()> {
            let path = resolve_path(path_arg);
            if path.exists() {
                self.multi
                    .println(dashboard::log_thinking(&format!(
                        "Running code review on {path_arg}: AST analysis, security, complexity."
                    )))
                    .ok();
                self.submit(AgentTask::CodeReview { path }).await
            } else {
                self.print_missing_path(&path);
                Ok(())
            }
        }

        async fn demo(&mut self) -> AppResult<()> {
            self.multi.println("").ok();
            self.multi
                .println(dashboard::log_thinking(
                    "Spawning 4 parallel agents to process real media fixtures.",
                ))
                .ok();
            self.multi
                .println(dashboard::log_thinking(
                    "Each agent runs independently — watch spinners and completions.",
                ))
                .ok();
            self.multi.println("").ok();

            let fix = fixture_dir();
            for task in demo_tasks(&fix) {
                self.submit(task).await?;
            }
            self.multi.println("").ok();
            Ok(())
        }

        fn cancel_task(&mut self, id_arg: &str) {
            if let Ok(id) = id_arg.parse::<usize>() {
                if let Some(handle) = self.handles.iter().find(|handle| handle.id == id) {
                    handle.cancel.cancel();
                    if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
                        task.status = TaskStatus::Cancelled;
                        task.message = "Cancelled by user".into();
                        self.multi.println(dashboard::log_cancel(task)).ok();
                    }
                } else {
                    self.multi
                        .println(format!("  \x1b[31m✗ No running task with ID {id}\x1b[0m"))
                        .ok();
                }
            }
        }

        fn show_status(&mut self, done_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Completion>) {
            while let Ok(comp) = done_rx.try_recv() {
                self.handle_completion(comp);
            }
            self.multi
                .println(dashboard::render_status_table(&self.tasks))
                .ok();
        }

        fn show_detail(&self, id_arg: &str) {
            if let Ok(id) = id_arg.parse::<usize>() {
                if let Some(task) = self.tasks.iter().find(|task| task.id == id) {
                    self.multi
                        .println(dashboard::render_task_details(task))
                        .ok();
                } else {
                    self.multi
                        .println(format!("  \x1b[31m✗ Unknown task ID:\x1b[0m {id}"))
                        .ok();
                }
            }
        }

        fn show_stats(&self) {
            let stats = self.pool.stats();
            let running = self.count_status(&TaskStatus::Running);
            let done = self.count_status(&TaskStatus::Done);
            let failed = self.count_status(&TaskStatus::Failed);
            let cancelled = self.count_status(&TaskStatus::Cancelled);
            self.multi
                .println(format!(
                    "\n  \x1b[1mWorker Pool\x1b[0m: {} active / {} capacity\n  \x1b[1mTasks\x1b[0m: {} running, {} done, {} failed, {} cancelled, {} total\n  \x1b[1mUptime\x1b[0m: {}\n",
                    stats.running,
                    stats.capacity,
                    running,
                    done,
                    failed,
                    cancelled,
                    self.tasks.len(),
                    format_duration(self.start_time.elapsed())
                ))
                .ok();
        }

        fn clear_completed(&mut self) {
            let before = self.tasks.len();
            self.tasks.retain(|task| task.status == TaskStatus::Running);
            self.handles
                .retain(|handle| self.tasks.iter().any(|task| task.id == handle.id));
            self.multi
                .println(format!(
                    "  \x1b[2mCleared {} completed task(s).\x1b[0m",
                    before - self.tasks.len()
                ))
                .ok();
        }

        fn quit_requested(&self) -> bool {
            let running = self.count_status(&TaskStatus::Running);
            if running > 0 {
                self.multi
                    .println(format!(
                        "  \x1b[33m⚠ Shutting down — cancelling {running} running task(s)\x1b[0m"
                    ))
                    .ok();
                for handle in &self.handles {
                    handle.cancel.cancel();
                }
            }
            true
        }

        fn unknown_command(&self, command: &str) {
            self.multi
                .println(format!(
                    "  \x1b[31m✗ Unknown command:\x1b[0m {command}\n  Type \x1b[1m/help\x1b[0m for commands."
                ))
                .ok();
        }

        async fn submit(&mut self, task: AgentTask) -> AppResult<()> {
            submit_task(
                &self.pool,
                &self.multi,
                &mut self.tasks,
                &mut self.handles,
                &self.done_tx,
                &self.prog_tx,
                &mut self.next_id,
                task,
            )
            .await
        }

        fn handle_completion(&mut self, comp: Completion) {
            if let Some(task) = self.tasks.iter_mut().find(|task| task.id == comp.id)
                && task.status == TaskStatus::Running
            {
                dashboard::record_completion(task, comp.result);
                self.multi.println(dashboard::log_result(task)).ok();
            }
            if let Some(pos) = self.handles.iter().position(|handle| handle.id == comp.id) {
                self.handles[pos].spinner.finish_and_clear();
                self.handles.remove(pos);
            }
        }

        fn handle_progress(&mut self, update: ProgressUpdate) {
            if let Some(task) = self.tasks.iter_mut().find(|task| task.id == update.id) {
                task.progress = Some((update.current, update.total));
                task.message = update.message;
            }
        }

        fn interrupt(&self) {
            self.multi.println("  \x1b[33m⚠ Interrupted\x1b[0m").ok();
            for handle in &self.handles {
                handle.cancel.cancel();
            }
        }

        async fn shutdown(self, cancel: &tokio_util::sync::CancellationToken) {
            cancel.cancel();
            for handle in &self.handles {
                handle.spinner.finish_and_clear();
            }
            let running = self.count_status(&TaskStatus::Running);
            if running > 0 {
                println!("  \x1b[2mWaiting for {running} task(s) to finish...\x1b[0m");
            }
            self.pool.shutdown().await.ok();
            println!("  \x1b[1;32m👋 Done.\x1b[0m");
        }

        fn print_missing_path(&self, path: &std::path::Path) {
            self.multi
                .println(format!(
                    "  \x1b[31m✗ File not found:\x1b[0m {}",
                    path.display()
                ))
                .ok();
        }

        fn print_status_line(&self) {
            let stats = self.pool.stats();
            let bar = dashboard::format_status_bar(
                &self.tasks,
                stats.running,
                stats.capacity,
                self.start_time.elapsed(),
            );
            println!("  {bar}");
        }

        fn count_status(&self, status: &TaskStatus) -> usize {
            self.tasks
                .iter()
                .filter(|task| task.status == *status)
                .count()
        }
    }

    pub(super) async fn run(cancel: tokio_util::sync::CancellationToken) -> AppResult<()> {
        let pool = Pool::new(
            Arc::new(AgentHandler),
            PoolConfig::new("agent-pool").with_size(POOL_SIZE),
        );
        let multi = MultiProgress::new();
        let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<Completion>();
        let (prog_tx, mut prog_rx) = tokio::sync::mpsc::unbounded_channel::<ProgressUpdate>();
        let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(1);

        spawn_stdin_reader(line_tx, cancel.clone());
        let mut state = AppRuntime::new(pool, multi, done_tx, prog_tx);
        state.print_intro();

        loop {
            tokio::select! {
                Some(line) = line_rx.recv() => {
                    if state.handle_line(&line, &mut done_rx).await? {
                        break;
                    }
                    print_prompt();
                }
                Some(comp) = done_rx.recv() => state.handle_completion(comp),
                Some(update) = prog_rx.recv() => state.handle_progress(update),
                () = cancel.cancelled() => {
                    state.interrupt();
                    break;
                }
            }
        }

        state.shutdown(&cancel).await;

        // Force exit — the blocking stdin thread can't be interrupted
        #[allow(clippy::disallowed_methods)]
        std::process::exit(0);
    }

    fn spawn_stdin_reader(
        line_tx: tokio::sync::mpsc::Sender<String>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        tokio::task::spawn_blocking(move || {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let mut buf = String::new();
                let read_result = {
                    let stdin = io::stdin();
                    stdin.lock().read_line(&mut buf)
                };
                match read_result {
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
    }

    fn demo_tasks(fix: &std::path::Path) -> Vec<AgentTask> {
        vec![
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
        ]
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
            ProgressStyle::with_template(
                "  {spinner:.cyan} \x1b[1mAgent #{prefix}\x1b[0m {wide_msg}",
            )
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
                            if event.kind == EventKind::Progress
                                && let Some(ref p) = event.progress
                            {
                                let total = p.total.unwrap_or(0);
                                let pct = progress_percent(p.current, total);
                                let step_msg = p.message.as_deref().unwrap_or("Working...");
                                event_spinner.set_message(format!(
                                    "({event_label}) \x1b[2m— {step_msg}\x1b[0m \x1b[36m[{pct}%]\x1b[0m"
                                ));
                                let _ = event_prog_tx.send(ProgressUpdate {
                                    id,
                                    current: p.current,
                                    total,
                                    message: step_msg.to_string(),
                                });
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
            });

            // Wait for final result
            let result = match handle.result().await {
                Ok(output) => Ok(output),
                Err(e) => Err(e.message().to_string()),
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

    fn print_prompt() {
        print!("\x1b[1;36m❯\x1b[0m ");
        io::stdout().flush().ok();
    }

    fn progress_percent(current: u64, total: u64) -> u64 {
        current.saturating_mul(100).checked_div(total).unwrap_or(0)
    }
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    interactive::run(cancel).await
}

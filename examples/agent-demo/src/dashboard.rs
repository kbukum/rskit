//! Dashboard — formatting for activity log, status table, and status bar.
//!
//! Two output styles:
//! - **Activity log**: Copilot-style colored bullets with indented sub-status
//! - **Structured views**: Tables (`/status`) and key-value (`/detail`)

use crate::tasks::TaskOutput;
use rskit_cli::{OutputKV, OutputTable};
use std::time::{Duration, Instant};

/// Tracks the state of a submitted task.
#[derive(Clone)]
pub struct TrackedTask {
    pub id: usize,
    pub label: String,
    pub status: TaskStatus,
    pub started: Instant,
    pub progress: Option<(u64, u64)>,
    pub message: String,
    pub result: Option<TaskOutput>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TaskStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running   => write!(f, "\x1b[33m⟳ Running\x1b[0m"),
            Self::Done      => write!(f, "\x1b[32m✓ Done\x1b[0m"),
            Self::Failed    => write!(f, "\x1b[31m✗ Failed\x1b[0m"),
            Self::Cancelled => write!(f, "\x1b[2m⊘ Cancelled\x1b[0m"),
        }
    }
}

// ── Activity Log (Copilot-style) ──────────────────────────────────────

/// Agent thinking/planning message (orange bullet).
pub fn log_thinking(msg: &str) -> String {
    format!("  \x1b[33m●\x1b[0m {msg}")
}

/// Task spawned — green bullet with "└ Running" sub-status.
pub fn log_spawn(id: usize, label: &str) -> String {
    format!(
        "  \x1b[32m●\x1b[0m \x1b[1mAgent #{id}\x1b[0m ({label})\n    \x1b[2m└ Running\x1b[0m"
    )
}

/// Task completed — green check with summary and duration.
pub fn log_complete(task: &TrackedTask) -> String {
    let dur = format_duration(task.started.elapsed());
    let summary = task
        .result
        .as_ref()
        .map(|r| r.summary.as_str())
        .unwrap_or("completed");
    format!(
        "  \x1b[32m✓\x1b[0m \x1b[1mAgent #{}\x1b[0m ({})\n    \x1b[2m└ Completed\x1b[0m — {} \x1b[2m({})\x1b[0m",
        task.id, task.label, summary, dur
    )
}

/// Task failed — red cross with error and duration.
pub fn log_fail(task: &TrackedTask) -> String {
    let dur = format_duration(task.started.elapsed());
    let msg = task
        .result
        .as_ref()
        .map(|r| r.summary.as_str())
        .unwrap_or("unknown error");
    format!(
        "  \x1b[31m✗\x1b[0m \x1b[1mAgent #{}\x1b[0m ({})\n    \x1b[2m└ Failed\x1b[0m — {} \x1b[2m({})\x1b[0m",
        task.id, task.label, msg, dur
    )
}

/// Task cancelled — dim with duration.
pub fn log_cancel(task: &TrackedTask) -> String {
    let dur = format_duration(task.started.elapsed());
    format!(
        "  \x1b[2m⊘\x1b[0m \x1b[1mAgent #{}\x1b[0m ({})\n    \x1b[2m└ Cancelled ({})\x1b[0m",
        task.id, task.label, dur
    )
}

/// Dispatch to the right log formatter based on task status.
pub fn log_result(task: &TrackedTask) -> String {
    match task.status {
        TaskStatus::Done => log_complete(task),
        TaskStatus::Failed => log_fail(task),
        TaskStatus::Cancelled => log_cancel(task),
        _ => String::new(),
    }
}

// ── Persistent Status Bar ─────────────────────────────────────────────

/// Format the persistent status bar (shown at bottom of terminal).
pub fn format_status_bar(
    tasks: &[TrackedTask],
    pool_active: usize,
    pool_capacity: usize,
    elapsed: Duration,
) -> String {
    let running = tasks.iter().filter(|t| t.status == TaskStatus::Running).count();
    let done = tasks.iter().filter(|t| t.status == TaskStatus::Done).count();
    let failed = tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
    let elapsed_str = format_duration(elapsed);

    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("\x1b[33m{running} running\x1b[0m"));
    }
    if done > 0 {
        parts.push(format!("\x1b[32m{done} done\x1b[0m"));
    }
    if failed > 0 {
        parts.push(format!("\x1b[31m{failed} failed\x1b[0m"));
    }

    let stats = if parts.is_empty() {
        "\x1b[2mready\x1b[0m".to_string()
    } else {
        parts.join(" \x1b[2m·\x1b[0m ")
    };

    format!(
        "\x1b[2m───\x1b[0m {stats} \x1b[2m│ pool: {pool_active}/{pool_capacity} │ {elapsed_str}\x1b[0m"
    )
}

// ── Structured Views (/status, /detail) ──────────────────────────────

/// Renders a table of all tracked tasks.
pub fn render_status_table(tasks: &[TrackedTask]) -> String {
    if tasks.is_empty() {
        return "  \x1b[2mNo tasks submitted yet.\x1b[0m".to_string();
    }

    let mut table = OutputTable::new(vec!["ID", "Task", "Status", "Progress", "Duration"]);
    for t in tasks {
        let elapsed = t.started.elapsed();
        let dur = format_duration(elapsed);
        let progress = match (&t.status, t.progress) {
            (TaskStatus::Done, _) => "100%".to_string(),
            (TaskStatus::Cancelled, _) => "—".to_string(),
            (_, Some((cur, total))) if total > 0 => format!("{}%", (cur * 100) / total),
            _ => {
                if t.message.is_empty() {
                    "—".to_string()
                } else {
                    t.message.clone()
                }
            }
        };
        table.add_row(vec![
            t.id.to_string(),
            t.label.clone(),
            t.status.to_string(),
            progress,
            dur,
        ]);
    }
    format!("{table}")
}

/// Render details of a specific task.
pub fn render_task_details(task: &TrackedTask) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n  \x1b[1mAgent #{}\x1b[0m — {}\n",
        task.id, task.label
    ));
    out.push_str(&format!("  Status: {}\n", task.status));
    out.push_str(&format!(
        "  Duration: {}\n",
        format_duration(task.started.elapsed())
    ));
    if !task.message.is_empty() {
        out.push_str(&format!("  Current step: {}\n", task.message));
    }
    if let Some(ref output) = task.result {
        out.push_str(&format!("  Summary: {}\n", output.summary));
        if !output.details.is_empty() {
            let mut kv = OutputKV::new();
            for (k, v) in &output.details {
                kv.add(k, v);
            }
            out.push_str(&format!("{kv}"));
        }
    }
    out
}

// ── Helpers ───────────────────────────────────────────────────────────

pub fn format_duration(d: Duration) -> String {
    if d.as_secs() >= 60 {
        format!(
            "{}m {:.1}s",
            d.as_secs() / 60,
            (d.as_secs() % 60) as f64 + d.subsec_millis() as f64 / 1000.0
        )
    } else if d.as_millis() >= 1000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: usize, label: &str, status: TaskStatus) -> TrackedTask {
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

    fn make_completed_task(id: usize, label: &str, summary: &str) -> TrackedTask {
        TrackedTask {
            id,
            label: label.to_string(),
            status: TaskStatus::Done,
            started: Instant::now(),
            progress: Some((10, 10)),
            message: "Done".into(),
            result: Some(TaskOutput {
                summary: summary.to_string(),
                details: vec![("Key".into(), "Value".into())],
            }),
        }
    }

    #[test]
    fn log_thinking_contains_message() {
        let msg = log_thinking("Planning the pipeline.");
        assert!(msg.contains("Planning the pipeline."));
        assert!(msg.contains("●"));
    }

    #[test]
    fn log_spawn_shows_id_and_label() {
        let msg = log_spawn(3, "Analyze photo.jpg");
        assert!(msg.contains("Agent #3"));
        assert!(msg.contains("Analyze photo.jpg"));
        assert!(msg.contains("Running"));
    }

    #[test]
    fn log_complete_shows_summary() {
        let task = make_completed_task(1, "Resize img.png", "200×200 (3.2 KB)");
        let msg = log_complete(&task);
        assert!(msg.contains("Agent #1"));
        assert!(msg.contains("Completed"));
        assert!(msg.contains("200×200 (3.2 KB)"));
    }

    #[test]
    fn log_fail_shows_error() {
        let mut task = make_task(2, "Pipeline bad.jpg", TaskStatus::Failed);
        task.result = Some(TaskOutput {
            summary: "unsupported format".into(),
            details: vec![],
        });
        let msg = log_fail(&task);
        assert!(msg.contains("Agent #2"));
        assert!(msg.contains("Failed"));
        assert!(msg.contains("unsupported format"));
    }

    #[test]
    fn log_cancel_shows_duration() {
        let task = make_task(5, "CodeReview main.rs", TaskStatus::Cancelled);
        let msg = log_cancel(&task);
        assert!(msg.contains("Agent #5"));
        assert!(msg.contains("Cancelled"));
    }

    #[test]
    fn log_result_dispatches_correctly() {
        let done = make_completed_task(1, "X", "ok");
        assert!(log_result(&done).contains("Completed"));

        let mut failed = make_task(2, "Y", TaskStatus::Failed);
        failed.result = Some(TaskOutput {
            summary: "err".into(),
            details: vec![],
        });
        assert!(log_result(&failed).contains("Failed"));

        let cancelled = make_task(3, "Z", TaskStatus::Cancelled);
        assert!(log_result(&cancelled).contains("Cancelled"));

        let running = make_task(4, "W", TaskStatus::Running);
        assert!(log_result(&running).is_empty());
    }

    #[test]
    fn format_status_bar_ready_when_empty() {
        let bar = format_status_bar(&[], 0, 4, Duration::from_secs(5));
        assert!(bar.contains("ready"));
        assert!(bar.contains("pool: 0/4"));
        assert!(bar.contains("5.0s"));
    }

    #[test]
    fn format_status_bar_shows_counts() {
        let tasks = vec![
            make_task(1, "A", TaskStatus::Running),
            make_task(2, "B", TaskStatus::Running),
            make_completed_task(3, "C", "ok"),
        ];
        let bar = format_status_bar(&tasks, 2, 4, Duration::from_secs(12));
        assert!(bar.contains("2 running"));
        assert!(bar.contains("1 done"));
        assert!(bar.contains("pool: 2/4"));
    }

    #[test]
    fn render_status_table_empty() {
        let table = render_status_table(&[]);
        assert!(table.contains("No tasks"));
    }

    #[test]
    fn render_status_table_with_tasks() {
        let tasks = vec![
            make_task(1, "Analyze photo.jpg", TaskStatus::Running),
            make_completed_task(2, "Resize img.png", "done"),
        ];
        let table = render_status_table(&tasks);
        assert!(table.contains("Analyze photo.jpg"));
        assert!(table.contains("Resize img.png"));
        assert!(table.contains("100%"));
    }

    #[test]
    fn render_task_details_shows_all_fields() {
        let task = make_completed_task(1, "Analyze photo.jpg", "image/jpeg, 35 KB");
        let details = render_task_details(&task);
        assert!(details.contains("Agent #1"));
        assert!(details.contains("Analyze photo.jpg"));
        assert!(details.contains("Done"));
        assert!(details.contains("image/jpeg, 35 KB"));
        assert!(details.contains("Key"));
        assert!(details.contains("Value"));
    }

    #[test]
    fn format_duration_formats_correctly() {
        assert_eq!(format_duration(Duration::from_millis(50)), "50ms");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5.0s");
    }
}

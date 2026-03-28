//! Task definitions — each task type demonstrates a different rskit feature.
//! Tasks run with deliberate delays so progress is visible in the UI.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSource, FileSink, TempFile, detect_mime, file_meta};
use rskit_media::executor::MediaExecutor;
use rskit_media::ops::{CropRegion, MediaOp, ResizeMode, ResizeOp, Rotation};
use rskit_media::spatial::Resolution;
use rskit_media_image::ImageProcessor;
use rskit_worker::{Event, Handler, Progress};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A task the agent can execute in the background.
#[derive(Debug, Clone)]
pub enum AgentTask {
    /// Analyze a file: detect MIME, size, metadata.
    Analyze { path: PathBuf },
    /// Resize an image to a target resolution.
    Resize { path: PathBuf, width: u32, height: u32 },
    /// Run a multi-step image processing pipeline.
    Pipeline { path: PathBuf },
    /// Simulate a long-running batch job with progress.
    BatchProcess { count: u32 },
    /// Simulate a code review agent scanning files.
    CodeReview { path: PathBuf },
}

impl std::fmt::Display for AgentTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Analyze { path } => write!(f, "Analyze {}", short_name(path)),
            Self::Resize { path, width, height } => {
                write!(f, "Resize {} → {}×{}", short_name(path), width, height)
            }
            Self::Pipeline { path } => write!(f, "Pipeline {}", short_name(path)),
            Self::BatchProcess { count } => write!(f, "Batch ({count} items)"),
            Self::CodeReview { path } => write!(f, "CodeReview {}", short_name(path)),
        }
    }
}

fn short_name(p: &PathBuf) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

/// Result from a completed agent task.
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub summary: String,
    pub details: Vec<(String, String)>,
}

/// The handler that rskit-worker calls for each task.
pub struct AgentHandler;

#[async_trait]
impl Handler<AgentTask, TaskOutput> for AgentHandler {
    async fn handle(
        &self,
        task: AgentTask,
        emit: mpsc::Sender<Event<TaskOutput>>,
        cancel: CancellationToken,
    ) -> AppResult<TaskOutput> {
        let task_id = uuid::Uuid::new_v4();
        let wid = "agent";

        match task {
            AgentTask::Analyze { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Opening file...", 200),
                    ("Reading bytes...", 300),
                    ("Computing file hash...", 250),
                    ("Detecting MIME type...", 0),   // actual work
                    ("Parsing file headers...", 200),
                    ("Gathering metadata...", 0),     // actual work
                    ("Analyzing structure...", 300),
                    ("Building report...", 200),
                ];
                let total = steps.len() as u64;

                for (i, &(msg, delay_ms)) in steps.iter().enumerate() {
                    emit_progress(&emit, task_id, wid, i as u64, total, msg).await;
                    check_cancelled(&cancel)?;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }

                let source = FileSource::from_path(&path);
                let mime = detect_mime(&source).await?;
                let meta = file_meta(&source).await?;

                let size = meta.size.unwrap_or(0);
                let size_str = format_size(size);
                let ext = meta.extension.clone().unwrap_or_default();

                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: format!("{} — {} ({})", short_name(&path), mime, size_str),
                    details: vec![
                        ("MIME".into(), mime),
                        ("Size".into(), size_str),
                        ("Extension".into(), ext),
                    ],
                })
            }

            AgentTask::Resize { path, width, height } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading source image...", 300),
                    ("Decoding pixel data...", 400),
                    ("Computing target dimensions...", 200),
                    ("Applying resize filter...", 0),   // actual work
                    ("Encoding output...", 350),
                    ("Writing to temp file...", 250),
                    ("Verifying output...", 0),          // actual work
                    ("Cleaning up...", 150),
                ];
                let total = steps.len() as u64;

                let tmp = TempFile::with_extension("jpg")?;
                let sink_path = tmp.path().to_path_buf();

                for (i, &(msg, delay_ms)) in steps.iter().enumerate() {
                    emit_progress(&emit, task_id, wid, i as u64, total, msg).await;
                    check_cancelled(&cancel)?;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }

                    // Do actual work at step 3 (resize)
                    if i == 3 {
                        let source = FileSource::from_path(&path);
                        let processor = ImageProcessor::new();
                        let ops = vec![MediaOp::Resize(ResizeOp {
                            resolution: Resolution::new(width, height),
                            mode: ResizeMode::Fit,
                        })];
                        let sink = FileSink::Path(sink_path.clone());
                        let _result = processor.execute(&source, &ops, Some(&sink)).await?;
                    }
                }

                // Verify output using the SAME temp file we wrote to
                let out_source = FileSource::from_path(&sink_path);
                let out_meta = file_meta(&out_source).await.ok();
                let out_size = out_meta
                    .and_then(|m| m.size)
                    .map(format_size)
                    .unwrap_or_else(|| "unknown".into());

                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: format!("Resized to {}×{} ({})", width, height, out_size),
                    details: vec![
                        ("Target".into(), format!("{}×{}", width, height)),
                        ("Mode".into(), "Fit".into()),
                        ("Output size".into(), out_size),
                    ],
                })
            }

            AgentTask::Pipeline { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading source image...", 300),
                    ("Analyzing dimensions...", 250),
                    ("Step 1/3: Resize to 400×400...", 0), // actual
                    ("Step 1/3: Resize complete", 200),
                    ("Step 2/3: Crop center 200×200...", 0), // actual
                    ("Step 2/3: Crop complete", 200),
                    ("Step 3/3: Rotate 90°...", 0), // actual
                    ("Step 3/3: Rotate complete", 200),
                    ("Optimizing output...", 350),
                    ("Verifying pipeline output...", 250),
                ];
                let total = steps.len() as u64;

                let source = FileSource::from_path(&path);
                let processor = ImageProcessor::new();
                let ops = vec![
                    MediaOp::Resize(ResizeOp {
                        resolution: Resolution::new(400, 400),
                        mode: ResizeMode::Fit,
                    }),
                    MediaOp::Crop(CropRegion::new(100, 100, 200, 200)),
                    MediaOp::Rotate(Rotation::Degrees90),
                ];

                for (i, &(msg, delay_ms)) in steps.iter().enumerate() {
                    emit_progress(&emit, task_id, wid, i as u64, total, msg).await;
                    check_cancelled(&cancel)?;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                    // Execute actual pipeline at step 2
                    if i == 2 {
                        let _result = processor.execute(&source, &ops, None).await?;
                    }
                }

                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: "Pipeline complete: resize → crop → rotate".into(),
                    details: vec![
                        ("Steps".into(), "Resize 400×400 → Crop 200×200 → Rotate 90°".into()),
                        ("Status".into(), "All 3 operations succeeded".into()),
                    ],
                })
            }

            AgentTask::BatchProcess { count } => {
                for i in 0..count {
                    check_cancelled(&cancel)?;
                    emit_progress(
                        &emit,
                        task_id,
                        wid,
                        i as u64,
                        count as u64,
                        &format!("Processing item {}/{count}", i + 1),
                    )
                    .await;
                    // Vary delays to look realistic (100-250ms per item)
                    let delay = 100 + (i % 7) * 25;
                    tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                }

                emit_progress(&emit, task_id, wid, count as u64, count as u64, "Done").await;

                Ok(TaskOutput {
                    summary: format!("Processed {count} items"),
                    details: vec![
                        ("Items".into(), count.to_string()),
                        ("Status".into(), "All succeeded".into()),
                    ],
                })
            }

            AgentTask::CodeReview { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading repository...", 400),
                    ("Scanning file tree...", 350),
                    ("Parsing source files...", 500),
                    ("Building AST...", 400),
                    ("Analyzing complexity...", 350),
                    ("Checking code style...", 300),
                    ("Scanning for bugs...", 450),
                    ("Reviewing security patterns...", 400),
                    ("Computing metrics...", 300),
                    ("Generating suggestions...", 350),
                    ("Building review report...", 250),
                    ("Formatting output...", 200),
                ];
                let total = steps.len() as u64;

                // Do some real file analysis along the way
                let source = FileSource::from_path(&path);
                let mime = detect_mime(&source).await.unwrap_or_else(|_| "unknown".into());
                let meta = file_meta(&source).await.ok();

                for (i, &(msg, delay_ms)) in steps.iter().enumerate() {
                    emit_progress(&emit, task_id, wid, i as u64, total, msg).await;
                    check_cancelled(&cancel)?;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }

                let size = meta.and_then(|m| m.size).unwrap_or(0);
                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: format!("Review complete — {} ({}, {})", short_name(&path), mime, format_size(size)),
                    details: vec![
                        ("File".into(), short_name(&path)),
                        ("Type".into(), mime),
                        ("Size".into(), format_size(size)),
                        ("Issues".into(), "0 critical, 2 warnings, 5 suggestions".into()),
                        ("Complexity".into(), "Medium (cyclomatic: 12)".into()),
                    ],
                })
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes > 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes > 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

async fn emit_progress(
    emit: &mpsc::Sender<Event<TaskOutput>>,
    task_id: uuid::Uuid,
    worker_id: &str,
    current: u64,
    total: u64,
    message: &str,
) {
    let p = Progress::new(current, Some(total)).with_message(message);
    let _ = emit.send(Event::progress(task_id, worker_id, p)).await;
}

fn check_cancelled(cancel: &CancellationToken) -> AppResult<()> {
    if cancel.is_cancelled() {
        Err(AppError::new(ErrorCode::Internal, "Task cancelled"))
    } else {
        Ok(())
    }
}

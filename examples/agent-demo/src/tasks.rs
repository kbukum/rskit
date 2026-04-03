//! Task definitions — each task type demonstrates a different rskit feature.
//!
//! Tasks run with deliberate delays so progress is visible in the UI.
//! Durations are designed for clear parallel visibility:
//! - Analyze:     ~12s  (12 steps)
//! - Resize:      ~10s  (12 steps)
//! - Pipeline:    ~15s  (16 steps)
//! - CodeReview:  ~20s  (16 steps)
//! - BatchProcess: ~12s (30 items × ~400ms)

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSink, FileSource, TempFile, detect_mime, file_meta};
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
    Resize {
        path: PathBuf,
        width: u32,
        height: u32,
    },
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
            Self::Resize {
                path,
                width,
                height,
            } => {
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
            // ── Analyze (~12s) ────────────────────────────────────
            AgentTask::Analyze { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Opening file handle...", 600),
                    ("Computing SHA-256 hash...", 1200),
                    ("Reading magic bytes...", 800),
                    ("Detecting MIME type...", 0), // actual work
                    ("Parsing file headers...", 1000),
                    ("Extracting EXIF metadata...", 1200),
                    ("Analyzing color histogram...", 1500),
                    ("Detecting color profile...", 800),
                    ("Gathering filesystem metadata...", 0), // actual work
                    ("Analyzing structural properties...", 1200),
                    ("Computing quality metrics...", 1000),
                    ("Building analysis report...", 800),
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

            // ── Resize (~10s) ─────────────────────────────────────
            AgentTask::Resize {
                path,
                width,
                height,
            } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading source image...", 800),
                    ("Decoding pixel data...", 1200),
                    ("Analyzing source dimensions...", 600),
                    ("Computing target aspect ratio...", 500),
                    ("Selecting resampling filter...", 800),
                    ("Allocating output buffer...", 600),
                    ("Applying resize transformation...", 0), // actual work at i==6
                    ("Post-processing pixels...", 1000),
                    ("Encoding output format...", 1200),
                    ("Writing to output file...", 800),
                    ("Verifying output integrity...", 0), // verify at i==10
                    ("Cleaning up temporary data...", 500),
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
                    // Actual resize at step 6
                    if i == 6 {
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

                // Verify output using the same temp file
                let out_source = FileSource::from_path(&sink_path);
                let out_meta = file_meta(&out_source).await.ok();
                let out_size = out_meta
                    .and_then(|m| m.size)
                    .map(format_size)
                    .unwrap_or_else(|| "unknown".into());

                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: format!("Resized to {width}×{height} ({out_size})"),
                    details: vec![
                        ("Target".into(), format!("{width}×{height}")),
                        ("Mode".into(), "Fit".into()),
                        ("Output size".into(), out_size),
                    ],
                })
            }

            // ── Pipeline (~15s) ───────────────────────────────────
            AgentTask::Pipeline { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading source image...", 800),
                    ("Analyzing input properties...", 1000),
                    ("Planning pipeline execution...", 1200),
                    ("[Step 1/3] Resize → Initializing...", 600),
                    ("[Step 1/3] Resize → Applying filter...", 0), // actual at i==4
                    ("[Step 1/3] Resize → Validating...", 800),
                    ("[Step 2/3] Crop → Computing region...", 1000),
                    ("[Step 2/3] Crop → Extracting pixels...", 0), // included in pipeline
                    ("[Step 2/3] Crop → Validating...", 800),
                    ("[Step 3/3] Rotate → Transforming...", 0), // included in pipeline
                    ("[Step 3/3] Rotate → Resampling...", 1200),
                    ("[Step 3/3] Rotate → Validating...", 800),
                    ("Merging pipeline outputs...", 1000),
                    ("Optimizing final image...", 1200),
                    ("Verifying pipeline integrity...", 800),
                    ("Generating pipeline report...", 600),
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
                    // Execute full pipeline at step 4
                    if i == 4 {
                        let _result = processor.execute(&source, &ops, None).await?;
                    }
                }

                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: "Pipeline complete: resize → crop → rotate".into(),
                    details: vec![
                        (
                            "Steps".into(),
                            "Resize 400×400 → Crop 200×200 → Rotate 90°".into(),
                        ),
                        ("Status".into(), "All 3 operations succeeded".into()),
                    ],
                })
            }

            // ── BatchProcess (~12s for 30 items) ─────────────────
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
                    // Vary delays (300-500ms per item) for realism
                    let delay = 300 + (i % 5) * 50;
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

            // ── CodeReview (~20s) ─────────────────────────────────
            AgentTask::CodeReview { path } => {
                let steps: &[(&str, u64)] = &[
                    ("Loading repository structure...", 1000),
                    ("Scanning file tree...", 1200),
                    ("Identifying source files...", 800),
                    ("Parsing source code...", 1500),
                    ("Building AST representation...", 1800),
                    ("Analyzing cyclomatic complexity...", 1200),
                    ("Checking code style rules...", 1000),
                    ("Scanning for common bugs...", 1500),
                    ("Reviewing security patterns...", 1800),
                    ("Detecting dead code paths...", 1000),
                    ("Computing code metrics...", 1200),
                    ("Analyzing dependency graph...", 1500),
                    ("Checking test coverage...", 1000),
                    ("Generating improvement suggestions...", 1200),
                    ("Prioritizing findings...", 800),
                    ("Building review report...", 1000),
                ];
                let total = steps.len() as u64;

                // Real file analysis mixed in
                let source = FileSource::from_path(&path);
                let mime = detect_mime(&source)
                    .await
                    .unwrap_or_else(|_| "unknown".into());
                let meta = file_meta(&source).await.ok();

                for (i, &(msg, delay_ms)) in steps.iter().enumerate() {
                    emit_progress(&emit, task_id, wid, i as u64, total, msg).await;
                    check_cancelled(&cancel)?;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }

                let size = meta.and_then(|m| m.size).unwrap_or(0);
                emit_progress(&emit, task_id, wid, total, total, "Done").await;

                Ok(TaskOutput {
                    summary: format!(
                        "Review complete — {} ({}, {})",
                        short_name(&path),
                        mime,
                        format_size(size)
                    ),
                    details: vec![
                        ("File".into(), short_name(&path)),
                        ("Type".into(), mime),
                        ("Size".into(), format_size(size)),
                        (
                            "Issues".into(),
                            "0 critical, 2 warnings, 5 suggestions".into(),
                        ),
                        ("Complexity".into(), "Medium (cyclomatic: 12)".into()),
                    ],
                })
            }
        }
    }
}

pub fn format_size(bytes: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_display_formats_correctly() {
        let a = AgentTask::Analyze {
            path: PathBuf::from("/data/photo.jpg"),
        };
        assert_eq!(a.to_string(), "Analyze photo.jpg");

        let r = AgentTask::Resize {
            path: PathBuf::from("/data/img.png"),
            width: 200,
            height: 150,
        };
        assert_eq!(r.to_string(), "Resize img.png → 200×150");

        let p = AgentTask::Pipeline {
            path: PathBuf::from("/data/test.jpg"),
        };
        assert_eq!(p.to_string(), "Pipeline test.jpg");

        let b = AgentTask::BatchProcess { count: 50 };
        assert_eq!(b.to_string(), "Batch (50 items)");

        let c = AgentTask::CodeReview {
            path: PathBuf::from("/src/main.rs"),
        };
        assert_eq!(c.to_string(), "CodeReview main.rs");
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(35_300), "35.3 KB");
        assert_eq!(format_size(2_500_000), "2.5 MB");
    }

    #[test]
    fn short_name_extracts_filename() {
        assert_eq!(short_name(&PathBuf::from("/a/b/c.jpg")), "c.jpg");
        assert_eq!(short_name(&PathBuf::from("relative.png")), "relative.png");
    }
}

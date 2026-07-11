//! Collector — orchestrates source → transform → target pipelines.
//!
//! Uses an event-driven worker pool for parallel source fetching:
//! - **Workers** pull sources from a shared channel, fetch items, apply transforms,
//!   save to disk, and send lightweight events back via mpsc.
//! - **Main loop** receives events, owns all mutable state (result, manifest, progress),
//!   and drives the progress callback from a single context (no shared mutexes).
//!
//! Supports incremental builds via `.manifest.json` caching.

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;

use crate::manifest::{CacheStatus, Manifest, SourceStats};
use crate::source::Source;
use crate::target::{PublishResult, Target};
use crate::transform::Transform;
use crate::{DatasetLimits, Label};

use futures_util::StreamExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Shared work receiver — workers take turns pulling the next source.
type WorkReceiver =
    Arc<tokio::sync::Mutex<mpsc::Receiver<(usize, Box<dyn Source>, Option<SourceStats>)>>>;

// ── Configuration ────────────────────────────────────────────────────────

/// Configuration for the collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Directory where dataset output and manifest files are written.
    pub output_dir: PathBuf,
    /// Maximum number of sources processed concurrently.
    pub concurrency: usize,
    /// Per-source timeout in seconds. Non-positive means no timeout.
    pub source_timeout_secs: f64,
    /// Ignore manifest cache and rebuild from sources.
    pub force: bool,
    /// Dataset streaming and materialization limits.
    pub limits: DatasetLimits,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("dataset_build"),
            concurrency: 4,
            source_timeout_secs: 600.0,
            force: false,
            limits: DatasetLimits::default(),
        }
    }
}

// ── Result ───────────────────────────────────────────────────────────────

/// Result of a collection run.
#[derive(Debug, Clone, Default)]
pub struct CollectorResult {
    /// Total items emitted or reused from cache.
    pub total_items: usize,
    /// Count of real-labeled items.
    pub real_count: usize,
    /// Count of AI-generated-labeled items.
    pub ai_count: usize,
    /// Per-source collection statistics.
    pub source_stats: std::collections::HashMap<String, SourceStats>,
    /// Source names skipped because a completed cache entry was available.
    pub cached_sources: Vec<String>,
    /// Results returned by publish targets.
    pub publish_results: Vec<PublishResult>,
    /// Wall-clock duration in seconds.
    pub duration_seconds: f64,
    /// Output directory used by the collector.
    pub output_dir: PathBuf,
}

// ── Worker Events ────────────────────────────────────────────────────────

/// How a source stream concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceOutcome {
    /// Source reached the end of its stream.
    Done,
    /// Source exceeded the configured timeout.
    TimedOut,
    /// Source stopped because cancellation was requested.
    Cancelled,
}

/// Events sent from worker tasks to the main event loop.
enum WorkerEvent {
    Started {
        index: usize,
        name: String,
        max_items: Option<usize>,
    },
    Progress {
        index: usize,
        count: usize,
    },
    Completed {
        index: usize,
        name: String,
        stats: SourceStats,
        cache_key: serde_json::Value,
        outcome: SourceOutcome,
    },
    Failed {
        index: usize,
        name: String,
        error: String,
        stats: SourceStats,
        cache_key: serde_json::Value,
    },
}

// ── Worker Context ───────────────────────────────────────────────────────

/// Shared immutable context cloned into each worker task.
struct WorkerContext {
    transforms: Arc<Vec<Box<dyn Transform>>>,
    file_counter: Arc<AtomicUsize>,
    real_dir: PathBuf,
    ai_dir: PathBuf,
    timeout_secs: f64,
    limits: DatasetLimits,
    cancel: CancellationToken,
    event_tx: mpsc::Sender<WorkerEvent>,
}

// ── Progress Callback ────────────────────────────────────────────────────

/// Callback protocol for reporting collection progress.
///
/// Called **only from the main event loop** — never from worker tasks.
/// This means implementations do NOT need interior mutability or `Sync`.
///
/// All methods have default no-op implementations, so you only override
/// what you need.
pub trait ProgressCallback: Send {
    /// Called when a source starts streaming.
    fn on_source_start(&self, _index: usize, _name: &str, _max_items: Option<usize>) {}
    /// Called after a source emits or resumes item progress.
    fn on_source_progress(&self, _index: usize, _count: usize) {}
    /// Called when a source finishes successfully or partially.
    fn on_source_done(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    /// Called when a source is skipped from manifest cache.
    fn on_source_cached(&self, _index: usize, _name: &str, _stats: &SourceStats) {}
    /// Called when source processing fails.
    fn on_source_error(&self, _index: usize, _name: &str, _error: &str) {}
    /// Called before publishing to a target.
    fn on_publish_start(&self, _target: &str) {}
    /// Called after a target publishes successfully.
    fn on_publish_done(&self, _target: &str, _result: &PublishResult) {}
    /// Called when target publishing fails.
    fn on_publish_error(&self, _target: &str, _error: &str) {}
}

/// No-op progress callback (all defaults).
pub struct NullProgress;
impl ProgressCallback for NullProgress {}

// ── Collector ────────────────────────────────────────────────────────────

/// Orchestrate data collection from sources through transforms to targets.
pub struct Collector {
    sources: Vec<Box<dyn Source>>,
    targets: Vec<Box<dyn Target>>,
    transforms: Arc<Vec<Box<dyn Transform>>>,
    config: CollectorConfig,
    progress: Box<dyn ProgressCallback>,
}

impl Collector {
    /// Create a collector from explicit source, transform, target, config, and progress contracts.
    pub fn new(
        sources: Vec<Box<dyn Source>>,
        transforms: Vec<Box<dyn Transform>>,
        targets: Vec<Box<dyn Target>>,
        config: CollectorConfig,
        progress: Box<dyn ProgressCallback>,
    ) -> Self {
        Self {
            sources,
            targets,
            transforms: Arc::new(transforms),
            config,
            progress,
        }
    }

    /// Execute the full collection pipeline with parallel source fetching.
    ///
    /// Internally spawns a worker pool that communicates via channels.
    /// The main loop owns all mutable state — no shared mutexes.
    pub async fn run(self, cancel: &CancellationToken) -> AppResult<CollectorResult> {
        let start = Instant::now();
        let cfg = &self.config;
        let out = &cfg.output_dir;

        // Prepare output directories
        let real_dir = out.join("real");
        let ai_dir = out.join("ai");
        prepare_output_dirs(real_dir.clone(), ai_dir.clone()).await?;

        let mut result = CollectorResult {
            output_dir: out.clone(),
            ..Default::default()
        };

        let total_sources = self.sources.len();

        // Load manifest for cache checking
        let mut manifest = if cfg.force {
            Manifest::default()
        } else {
            load_manifest(out.to_path_buf()).await?
        };

        // Separate cached vs to-fetch (partial sources are resumed, not skipped)
        let mut sources_to_fetch: Vec<(usize, Box<dyn Source>, Option<SourceStats>)> = Vec::new();

        for (idx, mut source) in self.sources.into_iter().enumerate() {
            if !cfg.force {
                let cache_key = source.cache_key();
                match manifest.cache_status(source.name(), &cache_key, source.max_items()) {
                    CacheStatus::Done(stats) => {
                        result.total_items += stats.total;
                        result.real_count += stats.real;
                        result.ai_count += stats.ai;
                        result
                            .source_stats
                            .insert(source.name().to_string(), stats.clone());
                        result.cached_sources.push(source.name().to_string());
                        self.progress.on_source_cached(idx, source.name(), &stats);
                        tracing::debug!(
                            source = source.name(),
                            total = stats.total,
                            "cached, skipping"
                        );
                        continue;
                    }
                    CacheStatus::Partial(stats) => {
                        tracing::debug!(
                            source = source.name(),
                            total = stats.total,
                            offset = stats.fetched_offset,
                            "partial cache, resuming"
                        );
                        source.set_resume_state(stats.fetched_offset, stats.total);
                        sources_to_fetch.push((idx, source, Some(stats)));
                    }
                    CacheStatus::NotCached => {
                        sources_to_fetch.push((idx, source, None));
                    }
                }
            } else {
                sources_to_fetch.push((idx, source, None));
            }
        }

        if !sources_to_fetch.is_empty() {
            // File counter past existing files
            let existing = count_existing_files(real_dir.clone(), ai_dir.clone()).await?;
            let file_counter = Arc::new(AtomicUsize::new(existing));

            // Channels: work distribution + event reporting
            let (work_tx, work_rx) =
                mpsc::channel::<(usize, Box<dyn Source>, Option<SourceStats>)>(total_sources);
            let work_rx = Arc::new(tokio::sync::Mutex::new(work_rx));
            let event_capacity = cfg
                .limits
                .stream_buffer
                .max(total_sources.saturating_mul(4))
                .max(1);
            let (event_tx, mut event_rx) = mpsc::channel::<WorkerEvent>(event_capacity);

            // Spawn worker pool
            let num_workers = cfg.concurrency.min(sources_to_fetch.len());
            let mut worker_handles = Vec::with_capacity(num_workers);

            for _ in 0..num_workers {
                let ctx = WorkerContext {
                    transforms: self.transforms.clone(),
                    file_counter: file_counter.clone(),
                    real_dir: real_dir.clone(),
                    ai_dir: ai_dir.clone(),
                    timeout_secs: cfg.source_timeout_secs,
                    limits: cfg.limits,
                    cancel: cancel.clone(),
                    event_tx: event_tx.clone(),
                };
                let rx = work_rx.clone();

                worker_handles.push(tokio::spawn(worker_loop(rx, ctx)));
            }

            // Drop our copy so the event channel closes when all workers finish
            drop(event_tx);

            // Send all sources into the work channel
            for (idx, source, resume_stats) in sources_to_fetch {
                // If the channel is full or closed, just break
                if work_tx.send((idx, source, resume_stats)).await.is_err() {
                    break;
                }
            }
            // Close the work channel — workers exit when drained
            drop(work_tx);

            // ── Main event loop ──────────────────────────────────────
            let mut completed_count = 0usize;
            let mut cancellation_seen = false;
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        match event {
                            Some(event) => {
                                let (completed, should_save) = handle_worker_event(
                                    event,
                                    &mut result,
                                    &mut manifest,
                                    self.progress.as_ref(),
                                );
                                if should_save
                                    && let Err(e) =
                                        save_manifest(out.to_path_buf(), manifest.clone()).await
                                {
                                    tracing::warn!(error = %e, "failed to save manifest");
                                }
                                if completed {
                                    completed_count += 1;
                                }
                            }
                            None => break, // All workers done, channel closed
                        }
                    }
                    _ = cancel.cancelled(), if !cancellation_seen => {
                        tracing::debug!(completed = completed_count, "cancelled, waiting for workers");
                        cancellation_seen = true;
                    }
                }
            }

            // Wait for all worker tasks to finish
            for handle in worker_handles {
                let _ = handle.await;
            }

            // Drain any remaining events after cancellation
            while let Ok(event) = event_rx.try_recv() {
                handle_drained_worker_event(
                    event,
                    &mut result,
                    &mut manifest,
                    self.progress.as_ref(),
                );
            }
        }

        // ── Publish to targets ───────────────────────────────────────
        for target in &self.targets {
            if cancel.is_cancelled() {
                break;
            }
            self.progress.on_publish_start(target.name());
            tracing::debug!(target = target.name(), "publishing");
            match target.publish(out, None).await {
                Ok(pub_result) => {
                    self.progress.on_publish_done(target.name(), &pub_result);
                    result.publish_results.push(pub_result);
                }
                Err(e) => {
                    self.progress
                        .on_publish_error(target.name(), &e.to_string());
                    tracing::error!(target = target.name(), error = %e, "publish failed");
                }
            }
        }

        result.duration_seconds = start.elapsed().as_secs_f64();

        // Save final manifest
        save_manifest(out.to_path_buf(), manifest).await?;

        tracing::debug!(
            total = result.total_items,
            real = result.real_count,
            ai = result.ai_count,
            duration = format!("{:.1}s", result.duration_seconds),
            "collection complete"
        );

        Ok(result)
    }
}

fn handle_worker_event(
    event: WorkerEvent,
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    progress: &dyn ProgressCallback,
) -> (bool, bool) {
    match event {
        WorkerEvent::Started {
            index,
            ref name,
            max_items,
        } => {
            progress.on_source_start(index, name, max_items);
            (false, false)
        }
        WorkerEvent::Progress { index, count } => {
            progress.on_source_progress(index, count);
            (false, false)
        }
        WorkerEvent::Completed {
            index,
            ref name,
            ref stats,
            ref cache_key,
            outcome,
        } => {
            record_completed_event(
                result,
                manifest,
                CompletedEventRef {
                    index,
                    name,
                    stats,
                    cache_key,
                    outcome,
                },
                progress,
            );
            (true, true)
        }
        WorkerEvent::Failed {
            index,
            ref name,
            ref error,
            ref stats,
            ref cache_key,
        } => {
            let saved = record_failed_event(
                result,
                manifest,
                FailedEventRef {
                    index,
                    name,
                    error,
                    stats,
                    cache_key,
                },
                progress,
            );
            (true, saved)
        }
    }
}

fn handle_drained_worker_event(
    event: WorkerEvent,
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    progress: &dyn ProgressCallback,
) {
    match event {
        WorkerEvent::Completed {
            index,
            ref name,
            ref stats,
            ref cache_key,
            outcome,
        } => record_completed_event(
            result,
            manifest,
            CompletedEventRef {
                index,
                name,
                stats,
                cache_key,
                outcome,
            },
            progress,
        ),
        WorkerEvent::Failed {
            index,
            ref name,
            ref error,
            ref stats,
            ref cache_key,
        } => {
            record_failed_event(
                result,
                manifest,
                FailedEventRef {
                    index,
                    name,
                    error,
                    stats,
                    cache_key,
                },
                progress,
            );
        }
        WorkerEvent::Started { .. } | WorkerEvent::Progress { .. } => {}
    }
}

struct CompletedEventRef<'a> {
    index: usize,
    name: &'a str,
    stats: &'a SourceStats,
    cache_key: &'a serde_json::Value,
    outcome: SourceOutcome,
}

struct FailedEventRef<'a> {
    index: usize,
    name: &'a str,
    error: &'a str,
    stats: &'a SourceStats,
    cache_key: &'a serde_json::Value,
}

fn record_completed_event(
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    event: CompletedEventRef<'_>,
    progress: &dyn ProgressCallback,
) {
    let CompletedEventRef {
        index,
        name,
        stats,
        cache_key,
        outcome,
    } = event;
    result.total_items += stats.total;
    result.real_count += stats.real;
    result.ai_count += stats.ai;
    result.source_stats.insert(name.to_string(), stats.clone());
    match outcome {
        SourceOutcome::Done => {
            manifest.mark_done(name.to_string(), cache_key.clone(), stats.clone())
        }
        SourceOutcome::TimedOut | SourceOutcome::Cancelled => {
            manifest.mark_partial(name.to_string(), cache_key.clone(), stats.clone());
        }
    }
    progress.on_source_done(index, name, stats);
}

fn record_failed_event(
    result: &mut CollectorResult,
    manifest: &mut Manifest,
    event: FailedEventRef<'_>,
    progress: &dyn ProgressCallback,
) -> bool {
    let FailedEventRef {
        index,
        name,
        error,
        stats,
        cache_key,
    } = event;
    result.source_stats.insert(name.to_string(), stats.clone());
    let saved = stats.total > 0;
    if saved {
        result.total_items += stats.total;
        result.real_count += stats.real;
        result.ai_count += stats.ai;
        manifest.mark_partial(name.to_string(), cache_key.clone(), stats.clone());
    }
    progress.on_source_error(index, name, error);
    saved
}

// ── Worker implementation ────────────────────────────────────────────────

/// Long-lived worker task — pulls sources from the shared work channel
/// and processes them sequentially until the channel is drained.
async fn worker_loop(work_rx: WorkReceiver, ctx: WorkerContext) {
    loop {
        // Lock the receiver just long enough to pull the next source
        let task = {
            let mut rx = work_rx.lock().await;
            rx.recv().await
        };
        match task {
            Some((idx, source, resume_stats)) => {
                process_source(idx, source, &ctx, resume_stats).await
            }
            None => break, // Channel closed, no more work
        }
    }
}

/// Process a single source: fetch items, apply transforms, save to disk,
/// and report events back to the main loop.
async fn process_source(
    idx: usize,
    source: Box<dyn Source>,
    ctx: &WorkerContext,
    resume_stats: Option<SourceStats>,
) {
    let name = source.name().to_string();
    let max_items = source.max_items();

    let resume_total = resume_stats.as_ref().map(|s| s.total).unwrap_or(0);
    let resume_real = resume_stats.as_ref().map(|s| s.real).unwrap_or(0);
    let resume_ai = resume_stats.as_ref().map(|s| s.ai).unwrap_or(0);

    // Notify main loop that we're starting
    send_worker_event(
        ctx,
        WorkerEvent::Started {
            index: idx,
            name: name.clone(),
            max_items,
        },
    )
    .await;

    // Send initial progress for resumed sources so the bar shows the starting point
    if resume_total > 0 {
        let _ = ctx.event_tx.try_send(WorkerEvent::Progress {
            index: idx,
            count: resume_total,
        });
    }

    tracing::debug!(source = name.as_str(), resume = resume_total, "fetching");

    let source_start = Instant::now();
    let timeout_secs = ctx.timeout_secs;
    let cache_key = source.cache_key();

    // Per-source counters — start from resume values so progress is cumulative.
    let mut total = resume_total;
    let mut real = resume_real;
    let mut ai = resume_ai;
    let mut fetched_offset = resume_stats.as_ref().map(|s| s.fetched_offset).unwrap_or(0);

    let mut stream = source.stream(ctx.cancel.clone());

    let process_stream = async {
        while let Some(item) = stream.next().await {
            if timeout_secs > 0.0 && source_start.elapsed().as_secs_f64() > timeout_secs {
                return Ok::<SourceOutcome, AppError>(SourceOutcome::TimedOut);
            }
            let item = item?;
            let pending_offset = item
                .source_offset()
                .unwrap_or_else(|| fetched_offset.saturating_add(1));
            let mut transformed = Some(item);
            for transform in ctx.transforms.iter() {
                transformed = match transformed {
                    Some(item) => transform.apply(item, &ctx.limits)?,
                    None => None,
                };
            }
            let Some(transformed) = transformed else {
                fetched_offset = pending_offset;
                continue;
            };

            let file_idx = ctx.file_counter.fetch_add(1, Ordering::SeqCst);
            let subdir = if transformed.label == Label::Real {
                &ctx.real_dir
            } else {
                &ctx.ai_dir
            };
            let path = subdir.join(format!("{:06}{}", file_idx, transformed.extension));
            let label = transformed.label;
            let limits = ctx.limits;
            tokio::task::spawn_blocking(move || transformed.write_to_path(&path, &limits))
                .await
                .map_err(AppError::internal)??;
            fetched_offset = pending_offset;

            if label == Label::Real {
                real += 1;
            } else {
                ai += 1;
            }
            total += 1;

            if !ctx.cancel.is_cancelled() {
                let _ = ctx.event_tx.try_send(WorkerEvent::Progress {
                    index: idx,
                    count: total,
                });
            }
        }
        Ok::<SourceOutcome, AppError>(SourceOutcome::Done)
    };

    // Consume with hard timeout via select.
    let mut timed_out = false;
    let fetch_result = if timeout_secs > 0.0 {
        tokio::select! {
            result = process_stream => result,
            _ = ctx.cancel.cancelled() => Ok(SourceOutcome::Cancelled),
            _ = tokio::time::sleep(Duration::from_secs_f64(timeout_secs)) => {
                timed_out = true;
                Ok(SourceOutcome::TimedOut)
            }
        }
    } else {
        tokio::select! {
            result = process_stream => result,
            _ = ctx.cancel.cancelled() => Ok(SourceOutcome::Cancelled),
        }
    };

    let stats = SourceStats {
        total,
        real,
        ai,
        fetched_offset,
    };

    // Report outcome to main loop
    match fetch_result {
        Ok(stream_outcome) => {
            let outcome = if ctx.cancel.is_cancelled() {
                SourceOutcome::Cancelled
            } else if timed_out {
                SourceOutcome::TimedOut
            } else {
                stream_outcome
            };
            send_worker_event(
                ctx,
                WorkerEvent::Completed {
                    index: idx,
                    name,
                    stats,
                    cache_key,
                    outcome,
                },
            )
            .await;
        }
        Err(e) => {
            let err_str = e.to_string();
            let short_err = if err_str.len() > 120 {
                format!("{}…", &err_str[..120])
            } else {
                err_str
            };
            send_worker_event(
                ctx,
                WorkerEvent::Failed {
                    index: idx,
                    name,
                    error: short_err,
                    stats,
                    cache_key,
                },
            )
            .await;
        }
    }
}

async fn send_worker_event(ctx: &WorkerContext, event: WorkerEvent) {
    let _ = ctx.event_tx.send(event).await;
}

async fn prepare_output_dirs(real_dir: PathBuf, ai_dir: PathBuf) -> AppResult<()> {
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&real_dir).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create real dir: {e}"),
            )
        })?;
        std::fs::create_dir_all(&ai_dir).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to create ai dir: {e}"))
        })
    })
    .await
    .map_err(AppError::internal)?
}

async fn load_manifest(output_dir: PathBuf) -> AppResult<Manifest> {
    tokio::task::spawn_blocking(move || Manifest::load(&output_dir))
        .await
        .map_err(AppError::internal)?
}

async fn save_manifest(output_dir: PathBuf, manifest: Manifest) -> AppResult<()> {
    tokio::task::spawn_blocking(move || manifest.save(&output_dir))
        .await
        .map_err(AppError::internal)?
}

async fn count_existing_files(real_dir: PathBuf, ai_dir: PathBuf) -> AppResult<usize> {
    tokio::task::spawn_blocking(move || Ok(count_files(&real_dir)? + count_files(&ai_dir)?))
        .await
        .map_err(AppError::internal)?
}

fn count_files(dir: &Path) -> AppResult<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    std::fs::read_dir(dir)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to read dataset output directory {}: {error}",
                    dir.display()
                ),
            )
        })
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .count()
        })
}

#[cfg(test)]
mod focused_tests {
    use super::*;
    use crate::{DataItem, MediaType};

    #[test]
    fn collector_config_default_and_null_progress_defaults_are_callable() {
        let cfg = CollectorConfig::default();
        assert_eq!(cfg.output_dir, PathBuf::from("dataset_build"));
        assert_eq!(cfg.concurrency, 4);
        assert_eq!(cfg.source_timeout_secs, 600.0);
        assert!(!cfg.force);
        assert_eq!(cfg.limits, DatasetLimits::default());

        let progress = NullProgress;
        let stats = SourceStats {
            total: 1,
            real: 1,
            ai: 0,
            fetched_offset: 1,
        };
        let published = PublishResult {
            target_name: "target".to_string(),
            location: "local".to_string(),
            files_published: 1,
            message: "ok".to_string(),
        };
        progress.on_source_start(0, "source", Some(1));
        progress.on_source_progress(0, 1);
        progress.on_source_done(0, "source", &stats);
        progress.on_source_cached(0, "source", &stats);
        progress.on_source_error(0, "source", "error");
        progress.on_publish_start("target");
        progress.on_publish_done("target", &published);
        progress.on_publish_error("target", "error");
    }

    #[test]
    fn count_files_ignores_directories_and_reports_read_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        assert_eq!(count_files(dir.path()).unwrap(), 1);
        assert_eq!(count_files(&dir.path().join("missing")).unwrap(), 0);

        let file = dir.path().join("not-dir");
        std::fs::write(&file, b"x").unwrap();
        let err = count_files(&file).unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(
            err.to_string()
                .contains("failed to read dataset output directory")
        );
    }

    struct StaticSource {
        name: &'static str,
        items: Vec<AppResult<DataItem>>,
        resume: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
        max_items: Option<usize>,
    }

    impl Source for StaticSource {
        fn name(&self) -> &str {
            self.name
        }

        fn stream(self: Box<Self>, _cancel: CancellationToken) -> crate::BoxDataStream {
            Box::pin(futures_util::stream::iter(self.items))
        }

        fn cache_key(&self) -> serde_json::Value {
            serde_json::json!({"name": self.name})
        }

        fn max_items(&self) -> Option<usize> {
            self.max_items.or(Some(self.items.len()))
        }

        fn set_resume_state(&mut self, offset: usize, _already_fetched: usize) {
            if let Some(resume) = &self.resume {
                resume.store(offset, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    struct DropNamedTransform;

    impl Transform for DropNamedTransform {
        fn name(&self) -> &str {
            "drop-named"
        }

        fn apply(&self, item: DataItem, _limits: &DatasetLimits) -> AppResult<Option<DataItem>> {
            if item.metadata.contains_key("drop") {
                Ok(None)
            } else {
                Ok(Some(item.with_extension("txt")))
            }
        }
    }

    struct PendingSource;

    impl Source for PendingSource {
        fn name(&self) -> &str {
            "pending"
        }

        fn stream(self: Box<Self>, _cancel: CancellationToken) -> crate::BoxDataStream {
            Box::pin(futures_util::stream::pending())
        }

        fn cache_key(&self) -> serde_json::Value {
            serde_json::json!("pending")
        }

        fn max_items(&self) -> Option<usize> {
            None
        }
    }

    struct CountingTarget;

    #[async_trait::async_trait]
    impl Target for CountingTarget {
        fn name(&self) -> &str {
            "counting"
        }

        async fn publish(
            &self,
            directory: &Path,
            _metadata: Option<&std::collections::HashMap<String, String>>,
        ) -> AppResult<PublishResult> {
            Ok(PublishResult {
                target_name: self.name().to_string(),
                location: directory.display().to_string(),
                files_published: count_files(&directory.join("real"))?
                    + count_files(&directory.join("ai"))?,
                message: "counted".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn collector_runs_sources_transforms_failures_and_targets() {
        let out = tempfile::tempdir().unwrap();
        let keep = DataItem::new(b"real".to_vec(), Label::Real, MediaType::Text, "ok")
            .unwrap()
            .with_source_offset(7);
        let drop = DataItem::new(b"skip".to_vec(), Label::AiGenerated, MediaType::Text, "ok")
            .unwrap()
            .with_metadata("drop", "yes");
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(StaticSource {
                name: "ok",
                items: vec![Ok(keep), Ok(drop)],
                resume: None,
                max_items: None,
            }),
            Box::new(StaticSource {
                name: "bad",
                items: vec![Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "bad item ".repeat(20),
                ))],
                resume: None,
                max_items: None,
            }),
        ];
        let collector = Collector::new(
            sources,
            vec![Box::new(DropNamedTransform)],
            vec![Box::new(CountingTarget)],
            CollectorConfig {
                output_dir: out.path().to_path_buf(),
                concurrency: 2,
                source_timeout_secs: 0.0,
                force: true,
                limits: DatasetLimits {
                    max_in_memory_bytes: 1024,
                    stream_buffer: 1,
                },
            },
            Box::new(NullProgress),
        );

        let result = collector.run(&CancellationToken::new()).await.unwrap();

        assert_eq!(result.total_items, 1);
        assert_eq!(result.real_count, 1);
        assert_eq!(result.ai_count, 0);
        assert!(result.source_stats.contains_key("ok"));
        assert!(result.source_stats.contains_key("bad"));
        assert_eq!(result.publish_results[0].files_published, 1);
        assert_eq!(count_files(&out.path().join("real")).unwrap(), 1);
    }

    #[tokio::test]
    async fn collector_skips_done_sources_and_resumes_partial_sources_from_manifest() {
        let out = tempfile::tempdir().unwrap();
        prepare_output_dirs(out.path().join("real"), out.path().join("ai"))
            .await
            .unwrap();
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "cached".to_string(),
            crate::manifest::SourceEntry {
                config: serde_json::json!({"name": "cached"}),
                stats: SourceStats {
                    total: 2,
                    real: 1,
                    ai: 1,
                    fetched_offset: 2,
                },
                status: "done".to_string(),
            },
        );
        manifest.sources.insert(
            "partial".to_string(),
            crate::manifest::SourceEntry {
                config: serde_json::json!({"name": "partial"}),
                stats: SourceStats {
                    total: 1,
                    real: 1,
                    ai: 0,
                    fetched_offset: 7,
                },
                status: "partial".to_string(),
            },
        );
        manifest.save(out.path()).unwrap();

        let resume = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let item = DataItem::new(
            b"ai".to_vec(),
            Label::AiGenerated,
            MediaType::Text,
            "partial",
        )
        .unwrap()
        .with_source_offset(8);
        let collector = Collector::new(
            vec![
                Box::new(StaticSource {
                    name: "cached",
                    items: Vec::new(),
                    resume: None,
                    max_items: None,
                }),
                Box::new(StaticSource {
                    name: "partial",
                    items: vec![Ok(item)],
                    resume: Some(std::sync::Arc::clone(&resume)),
                    max_items: Some(20),
                }),
            ],
            Vec::new(),
            Vec::new(),
            CollectorConfig {
                output_dir: out.path().to_path_buf(),
                concurrency: 1,
                source_timeout_secs: 0.0,
                force: false,
                limits: DatasetLimits {
                    max_in_memory_bytes: 1024,
                    stream_buffer: 1,
                },
            },
            Box::new(NullProgress),
        );

        let result = collector.run(&CancellationToken::new()).await.unwrap();

        assert_eq!(resume.load(std::sync::atomic::Ordering::SeqCst), 7);
        assert_eq!(result.cached_sources, ["cached"]);
        assert_eq!(result.total_items, 4);
        assert_eq!(result.real_count, 2);
        assert_eq!(result.ai_count, 2);
        assert!(result.source_stats.contains_key("cached"));
        assert!(result.source_stats.contains_key("partial"));
    }

    #[tokio::test(start_paused = true)]
    async fn collector_marks_hung_sources_partial_on_timeout() {
        let out = tempfile::tempdir().unwrap();
        let collector = Collector::new(
            vec![Box::new(PendingSource)],
            Vec::new(),
            Vec::new(),
            CollectorConfig {
                output_dir: out.path().to_path_buf(),
                concurrency: 1,
                source_timeout_secs: 1.0,
                force: true,
                limits: DatasetLimits {
                    max_in_memory_bytes: 1024,
                    stream_buffer: 1,
                },
            },
            Box::new(NullProgress),
        );
        let cancel = CancellationToken::new();
        let run = tokio::spawn(async move { collector.run(&cancel).await });

        tokio::time::advance(Duration::from_secs(1)).await;
        let result = run.await.unwrap().unwrap();

        assert_eq!(result.total_items, 0);
        let manifest = Manifest::load(out.path()).unwrap();
        assert!(matches!(
            manifest.cache_status("pending", &serde_json::json!("pending"), None),
            CacheStatus::NotCached
        ));
        assert!(result.source_stats.contains_key("pending"));
    }

    #[tokio::test]
    async fn worker_event_helpers_update_results_manifest_and_progress() {
        let mut result = CollectorResult::default();
        let mut manifest = Manifest::default();
        let progress = NullProgress;
        let cache_key = serde_json::json!({"source": "unit"});
        let stats = SourceStats {
            total: 3,
            real: 2,
            ai: 1,
            fetched_offset: 4,
        };

        assert_eq!(
            handle_worker_event(
                WorkerEvent::Started {
                    index: 0,
                    name: "unit".to_string(),
                    max_items: Some(10),
                },
                &mut result,
                &mut manifest,
                &progress,
            ),
            (false, false)
        );
        assert_eq!(
            handle_worker_event(
                WorkerEvent::Progress { index: 0, count: 2 },
                &mut result,
                &mut manifest,
                &progress,
            ),
            (false, false)
        );
        assert_eq!(
            handle_worker_event(
                WorkerEvent::Completed {
                    index: 0,
                    name: "unit".to_string(),
                    stats: stats.clone(),
                    cache_key: cache_key.clone(),
                    outcome: SourceOutcome::Done,
                },
                &mut result,
                &mut manifest,
                &progress,
            ),
            (true, true)
        );

        assert_eq!(result.total_items, 3);
        assert!(matches!(
            manifest.cache_status("unit", &cache_key, Some(3)),
            CacheStatus::Done(_)
        ));

        handle_drained_worker_event(
            WorkerEvent::Completed {
                index: 1,
                name: "cancelled".to_string(),
                stats: stats.clone(),
                cache_key: serde_json::json!("cancelled"),
                outcome: SourceOutcome::Cancelled,
            },
            &mut result,
            &mut manifest,
            &progress,
        );
        assert_eq!(result.total_items, 6);
        assert!(matches!(
            manifest.cache_status("cancelled", &serde_json::json!("cancelled"), Some(20)),
            CacheStatus::Partial(_)
        ));

        assert_eq!(
            handle_worker_event(
                WorkerEvent::Failed {
                    index: 2,
                    name: "failed".to_string(),
                    error: "boom".to_string(),
                    stats,
                    cache_key: serde_json::json!("failed"),
                },
                &mut result,
                &mut manifest,
                &progress,
            ),
            (true, true)
        );
        assert_eq!(result.total_items, 9);
        assert!(matches!(
            manifest.cache_status("failed", &serde_json::json!("failed"), Some(20)),
            CacheStatus::Partial(_)
        ));

        handle_drained_worker_event(
            WorkerEvent::Failed {
                index: 3,
                name: "empty-failed".to_string(),
                error: "empty".to_string(),
                stats: SourceStats::default(),
                cache_key: serde_json::json!("empty-failed"),
            },
            &mut result,
            &mut manifest,
            &progress,
        );
        assert_eq!(result.total_items, 9);
        assert!(result.source_stats.contains_key("empty-failed"));
    }

    #[tokio::test]
    async fn prepare_output_dirs_reports_real_and_ai_creation_errors() {
        let out = tempfile::tempdir().unwrap();
        let real_blocker = out.path().join("real-blocker");
        let ai_blocker = out.path().join("ai-blocker");
        std::fs::write(&real_blocker, b"x").unwrap();
        std::fs::write(&ai_blocker, b"x").unwrap();

        assert_eq!(
            prepare_output_dirs(real_blocker.join("child"), out.path().join("ai-ok"))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
        assert_eq!(
            prepare_output_dirs(out.path().join("real-ok"), ai_blocker.join("child"))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
    }
}

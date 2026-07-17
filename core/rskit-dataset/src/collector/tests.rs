use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;

use crate::manifest::{CacheStatus, Manifest, SourceStats};
use crate::sink::count_files;
use crate::source::Source;
use crate::target::{PublishResult, Target};
use crate::transform::Transform;
use crate::{DataItem, DatasetLimits, Label, LocalBlobSink, MediaType};

use super::config::CollectorConfig;
use super::engine::Collector;
use super::event::{SourceOutcome, WorkerEvent, handle_drained_worker_event, handle_worker_event};
use super::progress::{NullProgress, ProgressCallback};
use super::result::CollectorResult;

fn blob_sink(dir: &Path, limits: DatasetLimits) -> Arc<LocalBlobSink> {
    Arc::new(LocalBlobSink::new(dir, limits))
}

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

struct StaticSource {
    name: &'static str,
    items: Vec<AppResult<DataItem>>,
    resume: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    max_items: Option<usize>,
}

impl Source<DataItem> for StaticSource {
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

impl Transform<DataItem, DataItem> for DropNamedTransform {
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

impl Source<DataItem> for PendingSource {
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
    let sources: Vec<Box<dyn Source<DataItem>>> = vec![
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
    let limits = DatasetLimits {
        max_in_memory_bytes: 1024,
        stream_buffer: 1,
    };
    let collector = Collector::new(
        sources,
        vec![Box::new(DropNamedTransform)],
        blob_sink(out.path(), limits),
        vec![Box::new(CountingTarget)],
        CollectorConfig {
            output_dir: out.path().to_path_buf(),
            concurrency: 2,
            source_timeout_secs: 0.0,
            force: true,
            limits,
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
    let limits = DatasetLimits {
        max_in_memory_bytes: 1024,
        stream_buffer: 1,
    };
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
        blob_sink(out.path(), limits),
        Vec::new(),
        CollectorConfig {
            output_dir: out.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 0.0,
            force: false,
            limits,
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
    let limits = DatasetLimits {
        max_in_memory_bytes: 1024,
        stream_buffer: 1,
    };
    let collector = Collector::new(
        vec![Box::new(PendingSource)],
        Vec::new(),
        blob_sink(out.path(), limits),
        Vec::new(),
        CollectorConfig {
            output_dir: out.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 1.0,
            force: true,
            limits,
        },
        Box::new(NullProgress),
    );
    let cancel = CancellationToken::new();
    let run = tokio::spawn(async move { collector.run(&cancel).await });

    tokio::time::advance(std::time::Duration::from_secs(1)).await;
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
async fn collector_runs_a_single_worker_when_concurrency_is_zero() {
    let out = tempfile::tempdir().unwrap();
    let item = DataItem::new(b"real".to_vec(), Label::Real, MediaType::Text, "solo").unwrap();
    let limits = DatasetLimits {
        max_in_memory_bytes: 1024,
        stream_buffer: 1,
    };
    let collector = Collector::new(
        vec![Box::new(StaticSource {
            name: "solo",
            items: vec![Ok(item)],
            resume: None,
            max_items: None,
        })],
        Vec::new(),
        blob_sink(out.path(), limits),
        Vec::new(),
        CollectorConfig {
            output_dir: out.path().to_path_buf(),
            concurrency: 0,
            source_timeout_secs: 0.0,
            force: true,
            limits,
        },
        Box::new(NullProgress),
    );

    let result = collector.run(&CancellationToken::new()).await.unwrap();

    assert_eq!(result.total_items, 1);
    assert_eq!(result.real_count, 1);
    assert_eq!(count_files(&out.path().join("real")).unwrap(), 1);
}

#[tokio::test]
async fn collector_truncates_multibyte_source_errors_without_panicking() {
    let out = tempfile::tempdir().unwrap();
    // An error whose 120th byte falls in the middle of a multi-byte codepoint would panic a
    // byte-index slice; the source must still be recorded as failed.
    let message = format!("x{}", "€".repeat(60));
    assert!(message.len() > 120 && !message.is_char_boundary(120));
    let limits = DatasetLimits {
        max_in_memory_bytes: 1024,
        stream_buffer: 1,
    };
    let collector = Collector::new(
        vec![Box::new(StaticSource {
            name: "multibyte",
            items: vec![Err(AppError::new(ErrorCode::InvalidInput, message))],
            resume: None,
            max_items: None,
        })],
        Vec::new(),
        blob_sink(out.path(), limits),
        Vec::new(),
        CollectorConfig {
            output_dir: out.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 0.0,
            force: true,
            limits,
        },
        Box::new(NullProgress),
    );

    let result = collector.run(&CancellationToken::new()).await.unwrap();

    assert_eq!(result.total_items, 0);
    assert!(result.source_stats.contains_key("multibyte"));
}

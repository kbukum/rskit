use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use futures::stream;
use parking_lot::Mutex;
use rskit_dataset::{
    BoxDataStream, Collector, CollectorConfig, DataItem, DatasetLimits, Label, MediaType,
    ProgressCallback, PublishResult, Source, SourceStats, Target, Transform,
};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct FixtureSource {
    name: String,
    items: Vec<DataItem>,
    fail_after_items: bool,
    max_items: Option<usize>,
    resume_calls: Arc<Mutex<Vec<(usize, usize)>>>,
}

impl FixtureSource {
    fn new(name: &str, items: Vec<DataItem>) -> Self {
        Self {
            name: name.to_owned(),
            items,
            fail_after_items: false,
            max_items: None,
            resume_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing_after_items(mut self) -> Self {
        self.fail_after_items = true;
        self
    }

    fn with_max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }
}

impl Source for FixtureSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxDataStream {
        let mut events = self.items.into_iter().map(Ok).collect::<Vec<_>>();
        if self.fail_after_items {
            events.push(Err(AppError::new(
                ErrorCode::ExternalService,
                "source failed",
            )));
        }
        Box::pin(stream::iter(events))
    }

    fn cache_key(&self) -> serde_json::Value {
        json!({"fixture": self.name})
    }

    fn max_items(&self) -> Option<usize> {
        self.max_items.or(Some(self.items.len()))
    }

    fn set_resume_state(&mut self, offset: usize, already_fetched: usize) {
        self.resume_calls.lock().push((offset, already_fetched));
    }
}

struct ExtensionAndFilterTransform;

impl Transform for ExtensionAndFilterTransform {
    fn name(&self) -> &str {
        "extension-filter"
    }

    fn apply(&self, item: DataItem, _limits: &DatasetLimits) -> AppResult<Option<DataItem>> {
        if item
            .metadata
            .get("drop")
            .is_some_and(|value| value == "true")
        {
            return Ok(None);
        }
        if item
            .metadata
            .get("fail")
            .is_some_and(|value| value == "true")
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "transform rejected item",
            ));
        }
        Ok(Some(item.with_extension(".txt")))
    }
}

#[derive(Default)]
struct RecordingProgress {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingProgress {
    fn events(&self) -> Arc<Mutex<Vec<String>>> {
        self.events.clone()
    }
}

impl ProgressCallback for RecordingProgress {
    fn on_source_start(&self, index: usize, name: &str, max_items: Option<usize>) {
        self.events
            .lock()
            .push(format!("start:{index}:{name}:{max_items:?}"));
    }

    fn on_source_progress(&self, index: usize, count: usize) {
        self.events.lock().push(format!("progress:{index}:{count}"));
    }

    fn on_source_done(&self, index: usize, name: &str, stats: &SourceStats) {
        self.events
            .lock()
            .push(format!("done:{index}:{name}:{}", stats.total));
    }

    fn on_source_cached(&self, index: usize, name: &str, stats: &SourceStats) {
        self.events
            .lock()
            .push(format!("cached:{index}:{name}:{}", stats.total));
    }

    fn on_source_error(&self, index: usize, name: &str, error: &str) {
        self.events
            .lock()
            .push(format!("error:{index}:{name}:{error}"));
    }

    fn on_publish_start(&self, target: &str) {
        self.events.lock().push(format!("publish-start:{target}"));
    }

    fn on_publish_done(&self, target: &str, result: &PublishResult) {
        self.events
            .lock()
            .push(format!("publish-done:{target}:{}", result.files_published));
    }

    fn on_publish_error(&self, target: &str, error: &str) {
        self.events
            .lock()
            .push(format!("publish-error:{target}:{error}"));
    }
}

struct CountingTarget {
    name: &'static str,
    fail: bool,
}

struct DefaultResumeSource;

impl Source for DefaultResumeSource {
    fn name(&self) -> &str {
        "default-resume"
    }

    fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxDataStream {
        Box::pin(stream::empty())
    }

    fn cache_key(&self) -> serde_json::Value {
        json!({"source": "default-resume"})
    }

    fn max_items(&self) -> Option<usize> {
        None
    }
}

#[async_trait::async_trait]
impl Target for CountingTarget {
    fn name(&self) -> &str {
        self.name
    }

    async fn publish(
        &self,
        directory: &Path,
        _metadata: Option<&HashMap<String, String>>,
    ) -> AppResult<PublishResult> {
        if self.fail {
            return Err(AppError::new(ErrorCode::ExternalService, "publish failed"));
        }
        let files_published = ["real", "ai"]
            .into_iter()
            .map(|subdir| directory.join(subdir))
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter(|entry| entry.path().is_file())
            .count();
        Ok(PublishResult {
            target_name: self.name.to_owned(),
            location: directory.display().to_string(),
            files_published,
            message: "counted files".to_owned(),
        })
    }
}

fn item(bytes: &[u8], label: Label, source: &str, offset: usize) -> DataItem {
    DataItem::new_bytes(bytes.to_vec(), label, MediaType::Text, source)
        .unwrap()
        .with_source_offset(offset)
}

#[test]
fn source_display_name_uses_last_path_component_and_records_resume_state() {
    let mut source = FixtureSource::new("provider/path/name", Vec::new());
    assert_eq!(source.display_name(), "name");
    source.set_resume_state(10, 7);
    assert_eq!(*source.resume_calls.lock(), vec![(10, 7)]);

    let source = FixtureSource::new("plain", Vec::new());
    assert_eq!(source.display_name(), "plain");

    let mut default_resume = DefaultResumeSource;
    default_resume.set_resume_state(1, 1);
    assert_eq!(default_resume.display_name(), "default-resume");
}

#[tokio::test]
async fn collector_runs_transforms_publishes_targets_and_reuses_done_cache() {
    let dir = TempDir::new().unwrap();
    let progress = RecordingProgress::default();
    let events = progress.events();
    let source = FixtureSource::new(
        "fixture",
        vec![
            item(b"real", Label::Real, "fixture", 1),
            item(b"ai", Label::AiGenerated, "fixture", 2),
            item(b"drop", Label::Real, "fixture", 3).with_metadata("drop", "true"),
        ],
    );

    let result = Collector::new(
        vec![Box::new(source.clone())],
        vec![Box::new(ExtensionAndFilterTransform)],
        vec![
            Box::new(CountingTarget {
                name: "counting",
                fail: false,
            }),
            Box::new(CountingTarget {
                name: "failing",
                fail: true,
            }),
        ],
        CollectorConfig {
            output_dir: dir.path().to_path_buf(),
            concurrency: 2,
            source_timeout_secs: 0.0,
            force: false,
            limits: DatasetLimits {
                stream_buffer: 1,
                ..DatasetLimits::default()
            },
        },
        Box::new(progress),
    )
    .run(&CancellationToken::new())
    .await
    .unwrap();

    assert_eq!(result.total_items, 2);
    assert_eq!(result.real_count, 1);
    assert_eq!(result.ai_count, 1);
    assert_eq!(result.publish_results.len(), 1);
    assert_eq!(result.publish_results[0].files_published, 2);
    assert_eq!(
        std::fs::read(dir.path().join("real/000000.txt")).unwrap(),
        b"real"
    );
    assert_eq!(
        std::fs::read(dir.path().join("ai/000001.txt")).unwrap(),
        b"ai"
    );

    let observed = events.lock().clone();
    assert!(
        observed
            .iter()
            .any(|event| event == "start:0:fixture:Some(3)")
    );
    assert!(observed.iter().any(|event| event == "progress:0:1"));
    assert!(observed.iter().any(|event| event == "progress:0:2"));
    assert!(observed.iter().any(|event| event == "done:0:fixture:2"));
    assert!(
        observed
            .iter()
            .any(|event| event == "publish-start:counting")
    );
    assert!(
        observed
            .iter()
            .any(|event| event == "publish-done:counting:2")
    );
    assert!(
        observed
            .iter()
            .any(|event| event.starts_with("publish-error:failing:"))
    );

    let cached_progress = RecordingProgress::default();
    let cached_events = cached_progress.events();
    let cached_result = Collector::new(
        vec![Box::new(source)],
        Vec::new(),
        Vec::new(),
        CollectorConfig {
            output_dir: dir.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 0.0,
            force: false,
            limits: DatasetLimits::default(),
        },
        Box::new(cached_progress),
    )
    .run(&CancellationToken::new())
    .await
    .unwrap();

    assert_eq!(cached_result.total_items, 2);
    assert_eq!(cached_result.cached_sources, vec!["fixture"]);
    assert!(
        cached_events
            .lock()
            .iter()
            .any(|event| event == "cached:0:fixture:2")
    );
}

#[tokio::test]
async fn collector_records_partial_stats_for_source_and_transform_failures() {
    let dir = TempDir::new().unwrap();
    let progress = RecordingProgress::default();
    let events = progress.events();
    let source = FixtureSource::new(
        "partial",
        vec![
            item(b"real", Label::Real, "partial", 3),
            item(b"bad", Label::Real, "partial", 9).with_metadata("fail", "true"),
        ],
    )
    .failing_after_items()
    .with_max_items(20);

    let result = Collector::new(
        vec![Box::new(source.clone())],
        vec![Box::new(ExtensionAndFilterTransform)],
        Vec::new(),
        CollectorConfig {
            output_dir: dir.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 0.0,
            force: false,
            limits: DatasetLimits::default(),
        },
        Box::new(progress),
    )
    .run(&CancellationToken::new())
    .await
    .unwrap();

    assert_eq!(result.total_items, 1);
    assert_eq!(result.real_count, 1);
    assert_eq!(result.ai_count, 0);
    assert_eq!(result.source_stats["partial"].fetched_offset, 3);
    assert!(
        events
            .lock()
            .iter()
            .any(|event| event.starts_with("error:0:partial:"))
    );

    let resume_progress = RecordingProgress::default();
    let resume_source = source.clone();
    let resume_calls = resume_source.resume_calls.clone();
    let _ = Collector::new(
        vec![Box::new(resume_source)],
        Vec::new(),
        Vec::new(),
        CollectorConfig {
            output_dir: dir.path().to_path_buf(),
            concurrency: 1,
            source_timeout_secs: 0.0,
            force: false,
            limits: DatasetLimits::default(),
        },
        Box::new(resume_progress),
    )
    .run(&CancellationToken::new())
    .await
    .unwrap();

    assert_eq!(*resume_calls.lock(), vec![(3, 1)]);
}

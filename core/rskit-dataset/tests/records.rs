//! End-to-end collection of tabular records through the generic `Collector<DatasetRecord>`.
//!
//! Proves the engine is genuinely item-generic: the same collector that drives `DataItem` blob
//! samples also drives `DatasetRecord` rows, with pluggable schema validation, caching, resume, and
//! bounded concurrency.

use std::path::Path;
use std::sync::Arc;

use rskit_dataset::{
    Collector, CollectorConfig, DatasetLimits, JsonLinesReader, JsonLinesWriter, Manifest,
    NullProgress, RecordSink, RecordSource, SchemaValidator,
};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn write_source(dir: &Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

fn config(out: &Path, concurrency: usize, force: bool) -> CollectorConfig {
    CollectorConfig {
        output_dir: out.to_path_buf(),
        concurrency,
        source_timeout_secs: 0.0,
        force,
        limits: DatasetLimits {
            stream_buffer: 2,
            ..DatasetLimits::default()
        },
    }
}

fn schema_validator() -> Arc<SchemaValidator> {
    Arc::new(
        SchemaValidator::compile(&json!({
            "type": "object",
            "required": ["id"],
            "properties": { "id": {"type": "number"} }
        }))
        .unwrap(),
    )
}

#[tokio::test]
async fn collects_records_through_generic_collector_with_schema_validation() {
    let inputs = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("records.jsonl");

    let a = write_source(inputs.path(), "a.jsonl", &["{\"id\":1}", "{\"id\":2}"]);
    let b = write_source(inputs.path(), "b.jsonl", &["{\"id\":3}"]);

    let sink = Arc::new(RecordSink::new(Arc::new(JsonLinesWriter), &out_path));
    let result = Collector::new(
        vec![
            Box::new(RecordSource::new("a", Box::new(JsonLinesReader::new(&a))).with_max_items(2)),
            Box::new(RecordSource::new("b", Box::new(JsonLinesReader::new(&b))).with_max_items(1)),
        ],
        Vec::new(),
        sink,
        Vec::new(),
        config(out.path(), 2, true),
        Box::new(NullProgress),
    )
    .with_validator(schema_validator())
    .run(&CancellationToken::new())
    .await
    .unwrap();

    // Records default to the Real label, so all land in the real count.
    assert_eq!(result.total_items, 3);
    assert_eq!(result.real_count, 3);
    assert_eq!(result.ai_count, 0);

    let written = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(written.lines().filter(|line| !line.is_empty()).count(), 3);
    for id in ["\"id\":1", "\"id\":2", "\"id\":3"] {
        assert!(written.contains(id), "missing {id} in {written}");
    }
}

#[tokio::test]
async fn record_schema_validation_rejects_nonconforming_rows() {
    let inputs = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("records.jsonl");

    let bad = write_source(
        inputs.path(),
        "bad.jsonl",
        &["{\"id\":1}", "{\"id\":\"not-a-number\"}"],
    );

    let sink = Arc::new(RecordSink::new(Arc::new(JsonLinesWriter), &out_path));
    let result = Collector::new(
        vec![Box::new(RecordSource::new(
            "bad",
            Box::new(JsonLinesReader::new(&bad)),
        ))],
        Vec::new(),
        sink,
        Vec::new(),
        config(out.path(), 1, true),
        Box::new(NullProgress),
    )
    .with_validator(schema_validator())
    .run(&CancellationToken::new())
    .await
    .unwrap();

    // The first record is written; the second fails validation and the source is marked partial.
    assert_eq!(result.total_items, 1);
    assert!(result.source_stats.contains_key("bad"));

    let manifest = Manifest::load(out.path()).unwrap();
    assert!(matches!(
        manifest.cache_status("bad", &json!({"record-source": "bad"}), None),
        rskit_dataset::CacheStatus::Partial(_)
    ));
}

#[tokio::test]
async fn record_collection_resumes_from_cached_manifest() {
    let inputs = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();

    let source = write_source(inputs.path(), "s.jsonl", &["{\"id\":1}", "{\"id\":2}"]);

    let run = |force: bool| {
        let out_path = out.path().join("records.jsonl");
        let source = source.clone();
        let out_dir = out.path().to_path_buf();
        async move {
            Collector::new(
                vec![Box::new(
                    RecordSource::new("s", Box::new(JsonLinesReader::new(&source)))
                        .with_max_items(2),
                )],
                Vec::new(),
                Arc::new(RecordSink::new(Arc::new(JsonLinesWriter), &out_path)),
                Vec::new(),
                config(&out_dir, 1, force),
                Box::new(NullProgress),
            )
            .run(&CancellationToken::new())
            .await
            .unwrap()
        }
    };

    let first = run(true).await;
    assert_eq!(first.total_items, 2);

    // Second run without force: the source is fully cached and skipped.
    let second = run(false).await;
    assert_eq!(second.cached_sources, ["s"]);
    assert_eq!(second.total_items, 2);
}

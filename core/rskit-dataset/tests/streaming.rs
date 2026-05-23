use std::path::Path;

use futures::stream;
use futures_util::StreamExt as _;
use rskit_dataset::{
    BoxDataStream, CsvWriter, DataItem, DatasetLimits, DatasetReader, DatasetRecord, DatasetSchema,
    DatasetStreamExt, DatasetWriter, JsonLinesReader, JsonLinesWriter, Label, MediaType, Source,
    Target, Transform, filter_records, select_columns,
};
use rskit_errors::AppResult;
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

struct VecSource {
    items: Vec<DataItem>,
}

impl Source for VecSource {
    fn name(&self) -> &str {
        "vec"
    }

    fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxDataStream {
        Box::pin(stream::iter(self.items.into_iter().map(Ok)))
    }

    fn cache_key(&self) -> serde_json::Value {
        json!({"source": "vec"})
    }

    fn max_items(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

#[derive(Clone)]
struct ExtensionTransform;

impl Transform for ExtensionTransform {
    fn name(&self) -> &str {
        "extension"
    }

    fn apply(&self, item: DataItem, _limits: &DatasetLimits) -> AppResult<Option<DataItem>> {
        Ok(Some(item.with_extension(".dat")))
    }
}

#[tokio::test]
async fn source_stream_composes_with_dataset_pipeline_transform() {
    let source = VecSource {
        items: vec![
            DataItem::new_bytes(b"sample".to_vec(), Label::Real, MediaType::Text, "vec").unwrap(),
        ],
    };

    let results = Box::new(source)
        .stream(CancellationToken::new())
        .apply_dataset_transform(ExtensionTransform, DatasetLimits::default())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(results.len(), 1);
    let item = results.into_iter().next().unwrap().unwrap().unwrap();
    assert_eq!(item.extension, ".dat");
}

#[test]
fn file_payload_streams_to_destination_without_materializing() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.bin");
    let target_path = dir.path().join("target.bin");
    std::fs::write(&source_path, b"large-ish payload").unwrap();

    let item = DataItem::new_file(&source_path, Label::Real, MediaType::Text, "file")
        .with_extension(".bin");
    let written = item
        .write_to_path(&target_path, &DatasetLimits::default())
        .unwrap();

    assert_eq!(written, 17);
    assert_eq!(std::fs::read(target_path).unwrap(), b"large-ish payload");
}

#[test]
fn byte_payload_above_limit_is_rejected() {
    let limits = DatasetLimits {
        max_in_memory_bytes: 3,
        ..DatasetLimits::default()
    };

    let err =
        DataItem::new_bytes_with_limits(vec![0; 4], Label::Real, MediaType::Text, "bytes", &limits)
            .unwrap_err();
    assert!(err.to_string().contains("max_in_memory_bytes"));
}

#[test]
fn payload_cannot_be_constructed_above_default_limit() {
    let oversized = vec![0; DatasetLimits::default().max_in_memory_bytes + 1];
    let err = DataItem::new_bytes(oversized, Label::Real, MediaType::Text, "bytes").unwrap_err();
    assert!(err.to_string().contains("max_in_memory_bytes"));
}

#[test]
fn dataset_schema_delegates_record_validation_to_rskit_schema() {
    let schema = DatasetSchema::compile(&json!({
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": {"type": "string"}
        }
    }))
    .unwrap();

    assert!(schema.validate(&json!({"id": "ok"})).is_ok());
    assert!(schema.validate(&json!({"missing": true})).is_err());
}

#[tokio::test]
async fn local_target_counts_published_files() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("nested")).unwrap();
    std::fs::write(dir.path().join("nested/item.dat"), b"x").unwrap();

    let result = rskit_dataset::target::LocalTarget
        .publish(Path::new(dir.path()), None)
        .await
        .unwrap();

    assert_eq!(result.files_published, 1);
    assert_eq!(result.target_name, "local");
}

#[tokio::test]
async fn json_lines_records_select_filter_and_write_streaming() {
    use futures_util::StreamExt as _;

    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.jsonl");
    let output = dir.path().join("selected.jsonl");
    std::fs::write(
        &input,
        b"{\"id\":\"a\",\"keep\":true,\"drop\":1}\n{\"id\":\"b\",\"keep\":false,\"drop\":2}\n",
    )
    .unwrap();

    let records = Box::new(JsonLinesReader::new(&input)).stream();
    let filtered = filter_records(records, |record| {
        Ok(record
            .get("keep")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    });
    let selected = select_columns(filtered, vec!["id".to_string()]);
    let written = JsonLinesWriter
        .write(Box::pin(selected), output.as_path())
        .await
        .unwrap();

    assert_eq!(written, 1);

    let reread = Box::new(JsonLinesReader::new(&output))
        .stream()
        .collect::<Vec<_>>()
        .await;
    assert_eq!(
        reread.into_iter().next().unwrap().unwrap(),
        DatasetRecord::from_fields([("id", json!("a"))])
    );
}

#[tokio::test]
async fn csv_writer_rejects_records_with_different_columns() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("records.csv");
    let records = stream::iter([
        Ok(DatasetRecord::from_fields([("id", json!("a"))])),
        Ok(DatasetRecord::from_fields([
            ("id", json!("b")),
            ("extra", json!(true)),
        ])),
    ]);

    let err = CsvWriter
        .write(Box::pin(records), output.as_path())
        .await
        .unwrap_err();

    assert!(err.to_string().contains("columns do not match"));
}

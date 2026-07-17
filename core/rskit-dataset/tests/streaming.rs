use std::path::Path;

use futures::{FutureExt as _, stream};
use futures_util::StreamExt as _;
use rskit_dataset::{
    BoxDataStream, CsvReader, CsvWriter, DataItem, DataPayload, DatasetLimits, DatasetReader,
    DatasetRecord, DatasetSchema, DatasetStreamExt, DatasetWriter, JsonArrayReader,
    JsonLinesReader, JsonLinesWriter, Label, MediaType, Source, Target, Transform, filter_records,
    select_columns,
};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

struct VecSource {
    items: Vec<DataItem>,
}

impl Source<DataItem> for VecSource {
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

impl Transform<DataItem, DataItem> for ExtensionTransform {
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
fn file_payload_write_to_same_path_is_noop() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.bin");
    std::fs::write(&source_path, b"same file payload").unwrap();

    let item = DataItem::new_file(&source_path, Label::Real, MediaType::Text, "file")
        .with_extension(".bin");
    let written = item
        .write_to_path(&source_path, &DatasetLimits::default())
        .unwrap();

    assert_eq!(written, 17);
    assert_eq!(std::fs::read(source_path).unwrap(), b"same file payload");
}

#[cfg(unix)]
#[test]
fn file_payload_write_to_hard_link_is_noop() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.bin");
    let link_path = dir.path().join("link.bin");
    std::fs::write(&source_path, b"hard link payload").unwrap();
    std::fs::hard_link(&source_path, &link_path).unwrap();

    let item = DataItem::new_file(&source_path, Label::Real, MediaType::Text, "file")
        .with_extension(".bin");
    let written = item
        .write_to_path(&link_path, &DatasetLimits::default())
        .unwrap();

    assert_eq!(written, 17);
    assert_eq!(std::fs::read(source_path).unwrap(), b"hard link payload");
    assert_eq!(std::fs::read(link_path).unwrap(), b"hard link payload");
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
    assert!(rskit_dataset::validate_record(&schema, &json!({"id": "ok"})).is_ok());
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
async fn local_target_accepts_missing_directory_as_empty_publish() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing");

    let result = rskit_dataset::target::LocalTarget
        .publish(missing.as_path(), None)
        .await
        .unwrap();

    assert_eq!(result.files_published, 0);
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
async fn csv_reader_streams_header_mapped_records() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.csv");
    std::fs::write(&input, "id,label\na,real\nb,ai\n").unwrap();

    let records = Box::new(CsvReader::new(&input))
        .stream()
        .collect::<Vec<_>>()
        .await;

    assert_eq!(
        records.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
        vec![
            DatasetRecord::from_fields([("id", json!("a")), ("label", json!("real"))]),
            DatasetRecord::from_fields([("id", json!("b")), ("label", json!("ai"))]),
        ]
    );
}

#[test]
fn dataset_record_accessors_are_deterministic_and_project_missing_columns() {
    let record = DatasetRecord::from_fields([
        ("b", json!(2)),
        ("a", json!(1)),
        ("nested", json!({"ok": true})),
    ]);

    assert_eq!(
        record.fields().keys().cloned().collect::<Vec<_>>(),
        vec!["a", "b", "nested"]
    );
    assert_eq!(
        record
            .select(&["missing".to_string(), "b".to_string()])
            .into_fields(),
        [("b".to_string(), json!(2))].into_iter().collect()
    );
    assert_eq!(
        record.into_json(),
        json!({"a": 1, "b": 2, "nested": {"ok": true}})
    );
}

#[test]
fn dataset_readers_report_runtime_requirement_without_tokio() {
    let mut records = Box::new(JsonLinesReader::new("unused.jsonl")).stream();
    let err = records.next().now_or_never().unwrap().unwrap().unwrap_err();

    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.to_string().contains("Tokio runtime"));
}

#[tokio::test]
async fn csv_reader_rejects_oversized_records() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.csv");
    std::fs::write(&input, format!("id\n{}\n", "x".repeat(1024 * 1024))).unwrap();

    let mut records = Box::new(CsvReader::new(&input)).stream();
    let err = records.next().await.unwrap().unwrap_err();

    assert!(err.to_string().contains("exceeded max"));
    assert!(records.next().await.is_none());
}

#[tokio::test]
async fn csv_reader_rejects_missing_header_and_bad_paths() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.csv");
    std::fs::write(&empty, b"").unwrap();

    let mut records = Box::new(CsvReader::new(&empty)).stream();
    let err = records.next().await.unwrap().unwrap_err();
    assert!(err.to_string().contains("missing headers"));

    let mut missing = Box::new(CsvReader::new(dir.path().join("missing.csv"))).stream();
    let err = missing.next().await.unwrap().unwrap_err();
    assert!(err.to_string().contains("failed to open CSV"));
}

#[tokio::test]
async fn json_readers_reject_parse_errors_and_non_object_records() {
    let dir = TempDir::new().unwrap();
    let bad_lines = dir.path().join("bad.jsonl");
    let scalar_lines = dir.path().join("scalar.jsonl");
    let bad_array = dir.path().join("bad.json");
    let scalar_array = dir.path().join("scalar.json");
    std::fs::write(&bad_lines, b"{not json}\n").unwrap();
    std::fs::write(&scalar_lines, b"42\n").unwrap();
    std::fs::write(&bad_array, b"[{not json}]").unwrap();
    std::fs::write(&scalar_array, b"[42]").unwrap();

    let mut records = Box::new(JsonLinesReader::new(&bad_lines)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("parse")
    );

    let mut records = Box::new(JsonLinesReader::new(&scalar_lines)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("JSON object")
    );

    let mut records = Box::new(JsonArrayReader::new(&bad_array)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("parse")
    );

    let mut records = Box::new(JsonArrayReader::new(&scalar_array)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("JSON object")
    );
}

#[tokio::test]
async fn json_lines_reader_rejects_oversized_records() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.jsonl");
    std::fs::write(
        &input,
        format!("{{\"blob\":\"{}\"}}\n", "x".repeat(1024 * 1024)),
    )
    .unwrap();

    let mut records = Box::new(JsonLinesReader::new(&input)).stream();
    let err = records.next().await.unwrap().unwrap_err();

    assert!(err.to_string().contains("exceeded max"));
    assert!(records.next().await.is_none());
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

#[tokio::test]
async fn json_array_reader_rejects_oversized_fixture_files() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.json");
    std::fs::write(
        &input,
        format!("[{{\"blob\":\"{}\"}}]", "x".repeat(1024 * 1024)),
    )
    .unwrap();

    let mut records = Box::new(JsonArrayReader::new(&input)).stream();
    let err = records.next().await.unwrap().unwrap_err();

    assert!(err.to_string().contains("exceeded max"));
}

#[tokio::test]
async fn json_lines_reader_rejects_deeply_nested_records() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.jsonl");
    let nested = format!("{{\"value\":{}}}\n", "[".repeat(33) + "0" + &"]".repeat(33));
    std::fs::write(&input, nested).unwrap();

    let mut records = Box::new(JsonLinesReader::new(&input)).stream();
    let err = records.next().await.unwrap().unwrap_err();

    assert!(err.to_string().contains("nesting exceeds"));
}

#[tokio::test]
async fn json_lines_reader_rejects_records_with_too_many_fields() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("records.jsonl");
    let fields = (0..1025)
        .map(|idx| format!("\"field{idx}\":{idx}"))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(&input, format!("{{{fields}}}\n")).unwrap();

    let mut records = Box::new(JsonLinesReader::new(&input)).stream();
    let err = records.next().await.unwrap().unwrap_err();

    assert!(err.to_string().contains("fields"));
}

#[tokio::test]
async fn json_lines_reader_rejects_large_field_names_and_arrays() {
    let dir = TempDir::new().unwrap();

    let long_name = dir.path().join("long-name.jsonl");
    std::fs::write(&long_name, format!("{{\"{}\": true}}\n", "n".repeat(4097))).unwrap();
    let mut records = Box::new(JsonLinesReader::new(&long_name)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("field name")
    );

    let long_array = dir.path().join("long-array.jsonl");
    let items = std::iter::repeat_n("0", 16_385)
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(&long_array, format!("{{\"v\":[{items}]}}\n")).unwrap();
    let mut records = Box::new(JsonLinesReader::new(&long_array)).stream();
    assert!(
        records
            .next()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("array")
    );
}

#[tokio::test]
async fn writers_propagate_creation_and_input_errors() {
    let dir = TempDir::new().unwrap();
    let output_dir = dir.path().join("as-dir");
    std::fs::create_dir(&output_dir).unwrap();
    let records = stream::iter([Ok(DatasetRecord::from_fields([("id", json!("a"))]))]);

    let err = JsonLinesWriter
        .write(Box::pin(records), output_dir.as_path())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("failed to create JSON Lines"));

    let input_error = stream::iter([Err(AppError::new(ErrorCode::InvalidInput, "bad record"))]);
    let err = rskit_dataset::JsonArrayWriter
        .write(
            Box::pin(input_error),
            dir.path().join("array.json").as_path(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("bad record"));

    let csv_error = stream::iter([Err(AppError::new(ErrorCode::InvalidInput, "bad csv"))]);
    let err = CsvWriter
        .write(
            Box::pin(csv_error),
            dir.path().join("records.csv").as_path(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("bad csv"));
}

#[tokio::test]
async fn filter_records_preserves_input_and_predicate_errors() {
    let input_error = stream::iter([Err(AppError::new(ErrorCode::InvalidInput, "upstream"))]);
    let seen = filter_records(input_error, |_| Ok(true))
        .collect::<Vec<_>>()
        .await;
    assert_eq!(
        seen[0].as_ref().unwrap_err().code(),
        ErrorCode::InvalidInput
    );

    let records = stream::iter([Ok(DatasetRecord::from_fields([("id", json!("a"))]))]);
    let seen = filter_records(records, |_| {
        Err(AppError::new(ErrorCode::InvalidInput, "predicate"))
    })
    .collect::<Vec<_>>()
    .await;
    assert!(
        seen[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("predicate")
    );
}

#[tokio::test]
async fn json_array_writer_streams_records_without_buffering() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("records.json");
    let records = stream::iter([
        Ok(DatasetRecord::from_fields([("id", json!("a"))])),
        Ok(DatasetRecord::from_fields([("id", json!("b"))])),
    ]);

    let written = rskit_dataset::JsonArrayWriter
        .write(Box::pin(records), output.as_path())
        .await
        .unwrap();

    assert_eq!(written, 2);
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        r#"[{"id":"a"},{"id":"b"}]"#
    );
}

#[test]
fn data_item_carries_source_resume_offset() {
    let item = DataItem::new_bytes(b"x".to_vec(), Label::Real, MediaType::Text, "source")
        .unwrap()
        .with_source_offset(42);

    assert_eq!(item.source_offset(), Some(42));
}

#[test]
fn labels_and_media_types_have_stable_display_values() {
    assert_eq!(Label::Real.to_string(), "real");
    assert_eq!(Label::AiGenerated.to_string(), "ai");
    assert_eq!(MediaType::Image.to_string(), "image");
    assert_eq!(MediaType::Text.to_string(), "text");
    assert_eq!(MediaType::Audio.to_string(), "audio");
    assert_eq!(MediaType::Video.to_string(), "video");
}

#[test]
fn data_payload_reports_length_and_empty_state_for_bytes_and_files() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("payload.txt");
    std::fs::write(&path, b"file").unwrap();

    let bytes = DataPayload::bytes_default(Vec::new()).unwrap();
    assert!(bytes.is_bytes());
    assert!(bytes.is_empty().unwrap());
    assert_eq!(
        bytes.read_bytes_bounded(&DatasetLimits::default()).unwrap(),
        b""
    );

    let file = DataPayload::file(&path);
    assert!(!file.is_bytes());
    assert_eq!(file.as_file(), Some(path.as_path()));
    assert_eq!(file.len().unwrap(), 4);
    assert!(!file.is_empty().unwrap());
    assert_eq!(
        file.read_bytes_bounded(&DatasetLimits::default()).unwrap(),
        b"file"
    );

    assert_eq!(bytes.as_file(), None);
    assert_eq!(bytes.len().unwrap(), 0);
    assert_eq!(
        DataPayload::bytes(
            vec![1, 2, 3, 4],
            &DatasetLimits {
                max_in_memory_bytes: 8,
                ..DatasetLimits::default()
            }
        )
        .unwrap()
        .read_bytes_bounded(&DatasetLimits {
            max_in_memory_bytes: 3,
            ..DatasetLimits::default()
        })
        .unwrap_err()
        .code(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn file_payload_reading_is_bounded() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("payload.bin");
    std::fs::write(&path, b"too large").unwrap();
    let limits = DatasetLimits {
        max_in_memory_bytes: 3,
        ..DatasetLimits::default()
    };

    let err = DataPayload::file(&path)
        .read_bytes_bounded(&limits)
        .unwrap_err();

    assert!(err.to_string().contains("max_in_memory_bytes"));
}

#[test]
fn payload_file_errors_are_actionable_for_missing_sources_and_bad_targets() {
    let dir = TempDir::new().unwrap();
    let missing = DataPayload::file(dir.path().join("missing.bin"));
    assert!(
        missing
            .len()
            .unwrap_err()
            .to_string()
            .contains("stat payload")
    );
    assert!(
        missing
            .read_bytes_bounded(&DatasetLimits::default())
            .unwrap_err()
            .to_string()
            .contains("open payload")
    );
    assert!(
        missing
            .write_to_path(&dir.path().join("out.bin"), &DatasetLimits::default())
            .unwrap_err()
            .to_string()
            .contains("stat payload")
    );

    let source = dir.path().join("source.bin");
    let target_dir = dir.path().join("target-dir");
    std::fs::write(&source, b"file").unwrap();
    std::fs::create_dir(&target_dir).unwrap();
    let payload = DataPayload::file(&source);
    assert!(
        payload
            .write_to_path(&target_dir, &DatasetLimits::default())
            .unwrap_err()
            .to_string()
            .contains("create dataset item")
    );

    let source_dir = dir.path().join("source-dir");
    std::fs::create_dir(&source_dir).unwrap();
    let err = DataPayload::file(&source_dir)
        .write_to_path(&dir.path().join("from-dir.bin"), &DatasetLimits::default())
        .unwrap_err();
    assert!(err.to_string().contains("failed to stream payload"));
}

#[test]
fn byte_payload_write_to_path_success_and_failure_paths() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("bytes.bin");
    let payload = DataPayload::bytes_default(b"bytes".to_vec()).unwrap();

    assert_eq!(
        payload
            .write_to_path(&output, &DatasetLimits::default())
            .unwrap(),
        5
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"bytes");

    let err = payload
        .write_to_path(dir.path(), &DatasetLimits::default())
        .unwrap_err();
    assert!(err.to_string().contains("failed to write dataset item"));
}

#[test]
fn data_item_validate_rejects_unsafe_extensions_and_preserves_metadata() {
    let limits = DatasetLimits::default();
    let item = DataItem::new(
        b"x".to_vec(),
        Label::AiGenerated,
        MediaType::Image,
        "source",
    )
    .unwrap()
    .with_extension(".png")
    .with_metadata("prompt", "synthetic");

    assert!(item.validate(&limits).is_ok());
    assert_eq!(
        item.metadata.get("prompt").map(String::as_str),
        Some("synthetic")
    );

    let err = item
        .clone()
        .with_extension("../escape")
        .validate(&limits)
        .unwrap_err();
    assert!(err.to_string().contains("invalid dataset item extension"));

    assert!(item.clone().with_extension("txt").validate(&limits).is_ok());
    assert!(item.clone().with_extension(".").validate(&limits).is_err());
    assert!(item.clone().with_extension("..").validate(&limits).is_err());
    assert!(
        item.clone()
            .with_extension("a..b")
            .validate(&limits)
            .is_err()
    );
    assert!(
        item.clone()
            .with_extension(r"a\\b")
            .validate(&limits)
            .is_err()
    );
    assert_eq!(item.payload().len().unwrap(), 1);
}

#[test]
fn try_with_payload_rejects_oversized_in_memory_replacement() {
    let item = DataItem::new_bytes(b"x".to_vec(), Label::Real, MediaType::Text, "source").unwrap();
    let permissive = DatasetLimits {
        max_in_memory_bytes: 8,
        ..DatasetLimits::default()
    };
    let strict = DatasetLimits {
        max_in_memory_bytes: 3,
        ..DatasetLimits::default()
    };
    let payload = DataPayload::bytes(vec![0; 4], &permissive).unwrap();

    let err = item.try_with_payload(payload, &strict).unwrap_err();

    assert!(err.to_string().contains("max_in_memory_bytes"));
}

//! Streaming writers for structured dataset records.

use std::path::Path;

use futures_util::StreamExt as _;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;
use tokio::sync::mpsc;

use super::model::DatasetRecord;
use super::ops::BoxRecordStream;

/// Streaming writer for structured dataset records.
#[async_trait::async_trait]
pub trait DatasetWriter: Send + Sync {
    /// Write records to `path`, returning the number of records written.
    async fn write(&self, records: BoxRecordStream, path: &Path) -> AppResult<usize>;
}

/// CSV writer for structured records.
pub struct CsvWriter;

#[async_trait::async_trait]
impl DatasetWriter for CsvWriter {
    async fn write(&self, mut records: BoxRecordStream, path: &Path) -> AppResult<usize> {
        let (tx, mut rx) = mpsc::channel::<AppResult<DatasetRecord>>(8);
        let path = path.to_path_buf();
        let writer = tokio::task::spawn_blocking(move || -> AppResult<usize> {
            let mut writer = csv::Writer::from_path(&path).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create CSV dataset {}: {error}", path.display()),
                )
            })?;
            let mut headers: Option<Vec<String>> = None;
            let mut count = 0usize;
            while let Some(record) = rx.blocking_recv() {
                let record = record?;
                let columns =
                    headers.get_or_insert_with(|| record.fields().keys().cloned().collect());
                if count == 0 {
                    writer
                        .write_record(columns.iter())
                        .map_err(AppError::internal)?;
                } else {
                    ensure_csv_columns(columns, &record)?;
                }
                let row = columns
                    .iter()
                    .map(|column| {
                        record
                            .fields()
                            .get(column)
                            .map(value_to_cell)
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                writer.write_record(row).map_err(AppError::internal)?;
                count += 1;
            }
            writer.flush().map_err(AppError::internal)?;
            Ok(count)
        });

        while let Some(record) = records.next().await {
            if tx.send(record).await.is_err() {
                break;
            }
        }
        drop(tx);
        writer.await.map_err(AppError::internal)?
    }
}

/// JSON Lines writer for structured records.
pub struct JsonLinesWriter;

#[async_trait::async_trait]
impl DatasetWriter for JsonLinesWriter {
    async fn write(&self, mut records: BoxRecordStream, path: &Path) -> AppResult<usize> {
        let (tx, mut rx) = mpsc::channel::<AppResult<DatasetRecord>>(8);
        let path = path.to_path_buf();
        let writer = tokio::task::spawn_blocking(move || -> AppResult<usize> {
            use std::io::Write as _;

            let mut file = std::fs::File::create(&path).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "failed to create JSON Lines dataset {}: {error}",
                        path.display()
                    ),
                )
            })?;
            let mut count = 0usize;
            while let Some(record) = rx.blocking_recv() {
                let json = record?.into_json();
                serde_json::to_writer(&mut file, &json).map_err(AppError::internal)?;
                file.write_all(b"\n").map_err(AppError::internal)?;
                count += 1;
            }
            Ok(count)
        });

        while let Some(record) = records.next().await {
            if tx.send(record).await.is_err() {
                break;
            }
        }
        drop(tx);
        writer.await.map_err(AppError::internal)?
    }
}

/// JSON array writer for small fixture-style datasets.
pub struct JsonArrayWriter;

#[async_trait::async_trait]
impl DatasetWriter for JsonArrayWriter {
    async fn write(&self, mut records: BoxRecordStream, path: &Path) -> AppResult<usize> {
        let (tx, mut rx) = mpsc::channel::<AppResult<DatasetRecord>>(8);
        let path = path.to_path_buf();
        let writer = tokio::task::spawn_blocking(move || -> AppResult<usize> {
            use std::io::Write as _;

            let file = std::fs::File::create(&path).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create JSON dataset {}: {error}", path.display()),
                )
            })?;
            let mut writer = std::io::BufWriter::new(file);
            writer.write_all(b"[").map_err(AppError::internal)?;

            let mut count = 0usize;
            while let Some(record) = rx.blocking_recv() {
                if count > 0 {
                    writer.write_all(b",").map_err(AppError::internal)?;
                }
                serde_json::to_writer(&mut writer, &record?.into_json())
                    .map_err(AppError::internal)?;
                count += 1;
            }

            writer.write_all(b"]").map_err(AppError::internal)?;
            writer.flush().map_err(AppError::internal)?;
            Ok(count)
        });

        while let Some(record) = records.next().await {
            if tx.send(record).await.is_err() {
                break;
            }
        }
        drop(tx);
        writer.await.map_err(AppError::internal)?
    }
}

fn ensure_csv_columns(columns: &[String], record: &DatasetRecord) -> AppResult<()> {
    if record.fields().len() == columns.len()
        && columns
            .iter()
            .all(|column| record.fields().contains_key(column.as_str()))
    {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "CSV record columns do not match established header {:?}; record has {:?}",
            columns,
            record.fields().keys().collect::<Vec<_>>()
        ),
    ))
}

fn value_to_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use futures::stream;
    use serde_json::json;

    use super::*;

    #[test]
    fn private_writer_helpers_validate_columns_and_convert_cells() {
        assert!(
            ensure_csv_columns(
                &["a".to_string()],
                &DatasetRecord::from_fields([("a", json!(1))])
            )
            .is_ok()
        );
        assert!(
            ensure_csv_columns(
                &["a".to_string()],
                &DatasetRecord::from_fields([("b", json!(1))])
            )
            .is_err()
        );
        assert_eq!(value_to_cell(&Value::Null), "");
        assert_eq!(value_to_cell(&json!("text")), "text");
        assert_eq!(value_to_cell(&json!(true)), "true");
        assert_eq!(value_to_cell(&json!(42)), "42");
        assert_eq!(value_to_cell(&json!({"a": 1})), "{\"a\":1}");
        assert_eq!(value_to_cell(&json!([1, 2])), "[1,2]");
    }

    #[tokio::test]
    async fn writers_reject_directory_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let records = || {
            Box::pin(stream::iter(vec![Ok(DatasetRecord::from_fields([(
                "name",
                json!("a"),
            )]))])) as BoxRecordStream
        };

        assert!(CsvWriter.write(records(), dir.path()).await.is_err());
        assert!(JsonLinesWriter.write(records(), dir.path()).await.is_err());
        assert!(JsonArrayWriter.write(records(), dir.path()).await.is_err());
    }
}

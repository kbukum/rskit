//! Streaming readers for structured dataset records.

use std::path::{Path, PathBuf};

use futures::stream;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;
use tokio::sync::mpsc;

use super::limits::{
    MAX_CSV_RECORD_BYTES, MAX_JSON_ARRAY_BYTES, MAX_JSON_LINE_BYTES, validate_json_record,
};
use super::model::DatasetRecord;
use super::ops::BoxRecordStream;

/// Streaming reader for structured dataset records.
pub trait DatasetReader: Send + Sync + 'static {
    /// Convert this reader into a record stream.
    fn stream(self: Box<Self>) -> BoxRecordStream;
}

/// CSV record reader backed by the `csv` crate.
pub struct CsvReader {
    path: PathBuf,
}

impl CsvReader {
    /// Create a CSV reader for a local path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl DatasetReader for CsvReader {
    fn stream(self: Box<Self>) -> BoxRecordStream {
        let path = self.path;
        stream_from_blocking_reader(move |tx| {
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) => {
                    send_record(
                        &tx,
                        Err(AppError::new(
                            ErrorCode::Internal,
                            format!("failed to open CSV dataset {}: {error}", path.display()),
                        )),
                    );
                    return;
                }
            };
            let mut reader = std::io::BufReader::new(file);

            let headers = match read_line_bounded(&mut reader, MAX_CSV_RECORD_BYTES, "CSV header")
                .and_then(|line| {
                    line.map_or_else(
                        || {
                            Err(AppError::new(
                                ErrorCode::InvalidInput,
                                format!("CSV dataset {} is missing headers", path.display()),
                            ))
                        },
                        |line| parse_csv_record(&line),
                    )
                }) {
                Ok(headers) => headers,
                Err(error) => {
                    send_record(&tx, Err(error));
                    return;
                }
            };

            loop {
                match read_line_bounded(&mut reader, MAX_CSV_RECORD_BYTES, "CSV record")
                    .and_then(|line| line.map(|line| parse_csv_record(&line)).transpose())
                {
                    Ok(Some(raw)) => {
                        let fields = headers
                            .iter()
                            .zip(raw.iter())
                            .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                            .collect();
                        if !send_record(&tx, Ok(DatasetRecord::new(fields))) {
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(error) => {
                        send_record(&tx, Err(error));
                        return;
                    }
                }
            }
        })
    }
}

/// Newline-delimited JSON record reader.
pub struct JsonLinesReader {
    path: PathBuf,
}

impl JsonLinesReader {
    /// Create a JSON Lines reader for a local path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl DatasetReader for JsonLinesReader {
    fn stream(self: Box<Self>) -> BoxRecordStream {
        let path = self.path;
        stream_from_blocking_reader(move |tx| {
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) => {
                    send_record(
                        &tx,
                        Err(AppError::new(
                            ErrorCode::Internal,
                            format!(
                                "failed to open JSON Lines dataset {}: {error}",
                                path.display()
                            ),
                        )),
                    );
                    return;
                }
            };
            let mut reader = std::io::BufReader::new(file);
            loop {
                let record = match read_json_line_bounded(&mut reader) {
                    Ok(Some(line)) => record_from_json_bytes(&line),
                    Ok(None) => return,
                    Err(error) => {
                        send_record(&tx, Err(error));
                        return;
                    }
                };
                if !send_record(&tx, record) {
                    return;
                }
            }
        })
    }
}

/// Bounded JSON array reader for small fixture-style datasets.
pub struct JsonArrayReader {
    path: PathBuf,
}

impl JsonArrayReader {
    /// Create a JSON array reader for a local path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl DatasetReader for JsonArrayReader {
    fn stream(self: Box<Self>) -> BoxRecordStream {
        let path = self.path;
        stream_from_blocking_reader(move |tx| {
            let values =
                match read_bounded_file(&path, MAX_JSON_ARRAY_BYTES as usize).and_then(|bytes| {
                    serde_json::from_slice::<Vec<Value>>(&bytes).map_err(|error| {
                        AppError::new(
                            ErrorCode::InvalidInput,
                            format!("failed to parse JSON dataset {}: {error}", path.display()),
                        )
                    })
                }) {
                    Ok(values) => values,
                    Err(error) => {
                        send_record(&tx, Err(error));
                        return;
                    }
                };

            for value in values {
                if !send_record(&tx, record_from_value(value)) {
                    return;
                }
            }
        })
    }
}

fn stream_from_blocking_reader(
    producer: impl FnOnce(mpsc::Sender<AppResult<DatasetRecord>>) + Send + 'static,
) -> BoxRecordStream {
    if tokio::runtime::Handle::try_current().is_err() {
        return Box::pin(stream::once(async {
            Err(AppError::new(
                ErrorCode::Internal,
                "dataset readers require a Tokio runtime for blocking IO isolation",
            ))
        }));
    }

    let (tx, rx) = mpsc::channel(8);
    let handle = tokio::task::spawn_blocking(move || producer(tx));
    drop(handle);
    Box::pin(stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|item| (item, rx))
    }))
}

fn send_record(
    tx: &mpsc::Sender<AppResult<DatasetRecord>>,
    item: AppResult<DatasetRecord>,
) -> bool {
    tx.blocking_send(item).is_ok()
}

fn record_from_json_bytes(line: &[u8]) -> AppResult<DatasetRecord> {
    let value = serde_json::from_slice(line).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to parse JSON record: {error}"),
        )
    })?;
    record_from_value(value)
}

fn read_json_line_bounded(reader: &mut impl std::io::BufRead) -> AppResult<Option<Vec<u8>>> {
    read_line_bounded(reader, MAX_JSON_LINE_BYTES, "JSON Lines record")
}

fn read_line_bounded(
    reader: &mut impl std::io::BufRead,
    max_bytes: usize,
    label: &str,
) -> AppResult<Option<Vec<u8>>> {
    let mut line = Vec::new();

    loop {
        let available = reader.fill_buf().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read JSON line: {error}"),
            )
        })?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }

        let consumed = match available.iter().position(|byte| *byte == b'\n') {
            Some(pos) => {
                let end = pos + 1;
                append_line_chunk(&mut line, &available[..end], max_bytes, label)?;
                end
            }
            None => {
                append_line_chunk(&mut line, available, max_bytes, label)?;
                available.len()
            }
        };
        reader.consume(consumed);

        if line.last() == Some(&b'\n') {
            return Ok(Some(line));
        }
    }
}

fn append_line_chunk(
    line: &mut Vec<u8>,
    chunk: &[u8],
    max_bytes: usize,
    label: &str,
) -> AppResult<()> {
    if line.len().saturating_add(chunk.len()) > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{label} exceeded max {max_bytes} bytes"),
        ));
    }
    line.extend_from_slice(chunk);
    Ok(())
}

fn parse_csv_record(line: &[u8]) -> AppResult<csv::StringRecord> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(line);
    let mut records = reader.records();
    match records.next() {
        Some(Ok(record)) => Ok(record),
        Some(Err(error)) => Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to parse CSV record: {error}"),
        )),
        None => Err(AppError::new(
            ErrorCode::InvalidInput,
            "CSV record was empty",
        )),
    }
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> AppResult<Vec<u8>> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to open JSON dataset {}: {error}", path.display()),
        )
    })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read JSON dataset {}: {error}", path.display()),
            )
        })?;
    if bytes.len() > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "JSON array dataset {} exceeded max {MAX_JSON_ARRAY_BYTES} bytes while reading",
                path.display()
            ),
        ));
    }
    Ok(bytes)
}

fn record_from_value(value: Value) -> AppResult<DatasetRecord> {
    validate_json_record(&value)?;
    match value {
        Value::Object(fields) => Ok(DatasetRecord::new(fields.into_iter().collect())),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "dataset record must be a JSON object",
        )),
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt as _;
    use serde_json::json;

    use super::*;

    #[test]
    fn private_record_parsers_reject_invalid_shapes() {
        assert!(record_from_json_bytes(b"not-json").is_err());
        assert!(record_from_value(json!([1, 2])).is_err());
        assert!(parse_csv_record(b"").is_err());
        assert!(parse_csv_record(b"\xff").is_err());
    }

    #[test]
    fn private_bounded_readers_report_io_errors() {
        struct FailingRead;

        impl std::io::Read for FailingRead {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
        }

        impl std::io::BufRead for FailingRead {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                Err(std::io::Error::other("boom"))
            }

            fn consume(&mut self, _amt: usize) {}
        }

        let mut reader = FailingRead;
        assert_eq!(
            read_json_line_bounded(&mut reader).unwrap_err().code(),
            ErrorCode::Internal
        );

        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_bounded_file(dir.path(), 16).unwrap_err().code(),
            ErrorCode::Internal
        );
    }

    #[test]
    fn blocking_readers_report_missing_tokio_runtime() {
        let reader = JsonLinesReader::new("unused.jsonl");
        let mut stream = Box::new(reader).stream();
        let err = futures::executor::block_on(stream.next())
            .unwrap()
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.to_string().contains("Tokio runtime"));
    }

    #[tokio::test]
    async fn missing_record_readers_emit_errors_in_stream() {
        let missing = std::env::temp_dir().join("rskit-dataset-missing-record-file");
        let readers: Vec<Box<dyn DatasetReader>> = vec![
            Box::new(CsvReader::new(&missing)),
            Box::new(JsonLinesReader::new(&missing)),
            Box::new(JsonArrayReader::new(&missing)),
        ];

        for reader in readers {
            let mut stream = reader.stream();
            assert!(stream.next().await.unwrap().is_err());
        }
    }
}

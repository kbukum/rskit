//! Streaming row/record dataset abstractions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use futures::{Stream, stream};
use futures_util::StreamExt as _;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_pipeline::RskitStreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

/// Boxed stream of structured dataset records.
pub type BoxRecordStream = Pin<Box<dyn Stream<Item = AppResult<DatasetRecord>> + Send + 'static>>;

const MAX_JSON_ARRAY_BYTES: u64 = 1024 * 1024;
const MAX_JSON_LINE_BYTES: usize = 1024 * 1024;

/// Format identifier for built-in dataset record readers and writers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DatasetFormat {
    /// Comma-separated values with a header row.
    Csv,
    /// JSON array of records, intended for bounded fixture-style datasets.
    JsonArray,
    /// Newline-delimited JSON records.
    JsonLines,
}

/// Format-agnostic structured dataset row.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetRecord {
    fields: BTreeMap<String, Value>,
}

impl DatasetRecord {
    /// Create a record from named fields.
    #[must_use]
    pub fn new(fields: BTreeMap<String, Value>) -> Self {
        Self { fields }
    }

    /// Create a record from any iterator of named fields.
    #[must_use]
    pub fn from_fields<I, K>(fields: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Self {
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        }
    }

    /// Borrow a field by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    /// Borrow all record fields in deterministic key order.
    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    /// Consume this record into its fields.
    #[must_use]
    pub fn into_fields(self) -> BTreeMap<String, Value> {
        self.fields
    }

    /// Return a projected record with only the requested columns.
    #[must_use]
    pub fn select(&self, columns: &[String]) -> Self {
        let fields = columns
            .iter()
            .filter_map(|column| {
                self.fields
                    .get(column)
                    .map(|value| (column.clone(), value.clone()))
            })
            .collect();
        Self { fields }
    }

    /// Convert this record to a JSON object.
    #[must_use]
    pub fn into_json(self) -> Value {
        Value::Object(self.fields.into_iter().collect())
    }
}

/// Streaming reader for structured dataset records.
pub trait DatasetReader: Send + 'static {
    /// Convert this reader into a record stream.
    fn stream(self: Box<Self>) -> BoxRecordStream;
}

/// Streaming writer for structured dataset records.
#[async_trait::async_trait]
pub trait DatasetWriter: Send + Sync {
    /// Write records to `path`, returning the number of records written.
    async fn write(&self, records: BoxRecordStream, path: &Path) -> AppResult<usize>;
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
            let mut reader = match csv::Reader::from_path(&path) {
                Ok(reader) => reader,
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

            let headers = match reader.headers() {
                Ok(headers) => headers.clone(),
                Err(error) => {
                    send_record(
                        &tx,
                        Err(AppError::new(
                            ErrorCode::InvalidInput,
                            format!(
                                "failed to read CSV headers from {}: {error}",
                                path.display()
                            ),
                        )),
                    );
                    return;
                }
            };

            let mut raw = csv::StringRecord::new();
            loop {
                match reader.read_record(&mut raw) {
                    Ok(true) => {
                        let fields = headers
                            .iter()
                            .zip(raw.iter())
                            .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                            .collect();
                        if !send_record(&tx, Ok(DatasetRecord::new(fields))) {
                            return;
                        }
                    }
                    Ok(false) => return,
                    Err(error) => {
                        send_record(
                            &tx,
                            Err(AppError::new(
                                ErrorCode::InvalidInput,
                                format!("failed to read CSV record: {error}"),
                            )),
                        );
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
                    Err(error) => Err(error),
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
            let size = match std::fs::metadata(&path) {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    send_record(
                        &tx,
                        Err(AppError::new(
                            ErrorCode::Internal,
                            format!("failed to stat JSON dataset {}: {error}", path.display()),
                        )),
                    );
                    return;
                }
            };
            if size > MAX_JSON_ARRAY_BYTES {
                send_record(
                    &tx,
                    Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!(
                            "JSON array dataset {} is {size} bytes, exceeding max {MAX_JSON_ARRAY_BYTES}",
                            path.display()
                        ),
                    )),
                );
                return;
            }

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
                    headers.get_or_insert_with(|| record.fields.keys().cloned().collect());
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
                            .fields
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

fn ensure_csv_columns(columns: &[String], record: &DatasetRecord) -> AppResult<()> {
    if record.fields.len() == columns.len()
        && columns
            .iter()
            .all(|column| record.fields.contains_key(column.as_str()))
    {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "CSV record columns do not match established header {:?}; record has {:?}",
            columns,
            record.fields.keys().collect::<Vec<_>>()
        ),
    ))
}

fn stream_from_blocking_reader(
    producer: impl FnOnce(mpsc::Sender<AppResult<DatasetRecord>>) + Send + 'static,
) -> BoxRecordStream {
    let (tx, rx) = mpsc::channel(8);
    std::thread::spawn(move || producer(tx));
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

/// Project a record stream to the requested columns.
pub fn select_columns<S>(
    records: S,
    columns: Vec<String>,
) -> impl Stream<Item = AppResult<DatasetRecord>>
where
    S: Stream<Item = AppResult<DatasetRecord>> + Send + 'static,
{
    records.rmap(move |record| {
        let columns = columns.clone();
        async move { record.map(|record| record.select(&columns)) }
    })
}

/// Filter a record stream with a fallible predicate.
pub fn filter_records<S, F>(
    records: S,
    predicate: F,
) -> impl Stream<Item = AppResult<DatasetRecord>>
where
    S: Stream<Item = AppResult<DatasetRecord>> + Send + 'static,
    F: Fn(&DatasetRecord) -> AppResult<bool> + Clone + Send + Sync + 'static,
{
    records.filter_map(move |record| {
        let predicate = predicate.clone();
        async move {
            match record {
                Ok(record) => match predicate(&record) {
                    Ok(true) => Some(Ok(record)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error)),
                },
                Err(error) => Some(Err(error)),
            }
        }
    })
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
                append_json_line_chunk(&mut line, &available[..end])?;
                end
            }
            None => {
                append_json_line_chunk(&mut line, available)?;
                available.len()
            }
        };
        reader.consume(consumed);

        if line.last() == Some(&b'\n') {
            return Ok(Some(line));
        }
    }
}

fn append_json_line_chunk(line: &mut Vec<u8>, chunk: &[u8]) -> AppResult<()> {
    if line.len().saturating_add(chunk.len()) > MAX_JSON_LINE_BYTES {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("JSON Lines record exceeded max {MAX_JSON_LINE_BYTES} bytes"),
        ));
    }
    line.extend_from_slice(chunk);
    Ok(())
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
    match value {
        Value::Object(fields) => Ok(DatasetRecord::new(fields.into_iter().collect())),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "dataset record must be a JSON object",
        )),
    }
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

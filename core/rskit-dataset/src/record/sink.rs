//! Record sink adapter — materialize records through a [`DatasetWriter`] as an [`ItemSink`].

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult, ErrorCode};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::sink::ItemSink;

use super::model::DatasetRecord;
use super::ops::BoxRecordStream;
use super::writer::DatasetWriter;

/// Bounded buffer between the engine's workers and the background writer task.
const RECORD_CHANNEL_CAPACITY: usize = 128;

/// Adapts any [`DatasetWriter`] into an [`ItemSink`] that streams records to one dataset file.
///
/// Records are forwarded over a bounded channel to a background writer task started in
/// [`prepare`](ItemSink::prepare), preserving backpressure; [`finish`](ItemSink::finish) closes the
/// channel and awaits the writer.
pub struct RecordSink {
    writer: Arc<dyn DatasetWriter>,
    path: PathBuf,
    state: Mutex<Option<SinkState>>,
}

struct SinkState {
    tx: mpsc::Sender<AppResult<DatasetRecord>>,
    handle: JoinHandle<AppResult<usize>>,
}

impl RecordSink {
    /// Create a sink that writes records to `path` using `writer`.
    #[must_use]
    pub fn new(writer: Arc<dyn DatasetWriter>, path: impl Into<PathBuf>) -> Self {
        Self {
            writer,
            path: path.into(),
            state: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ItemSink<DatasetRecord> for RecordSink {
    fn name(&self) -> &str {
        "record"
    }

    async fn prepare(&self) -> AppResult<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            let parent = parent.to_path_buf();
            tokio::task::spawn_blocking(move || {
                std::fs::create_dir_all(&parent).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "failed to create dataset directory {}: {error}",
                            parent.display()
                        ),
                    )
                })
            })
            .await
            .map_err(AppError::internal)??;
        }

        let (tx, mut rx) = mpsc::channel::<AppResult<DatasetRecord>>(RECORD_CHANNEL_CAPACITY);
        let stream: BoxRecordStream =
            Box::pin(futures::stream::poll_fn(move |cx| rx.poll_recv(cx)));
        let writer = self.writer.clone();
        let path = self.path.clone();
        let handle = tokio::spawn(async move { writer.write(stream, &path).await });
        *self.state.lock().await = Some(SinkState { tx, handle });
        Ok(())
    }

    async fn write(&self, item: DatasetRecord) -> AppResult<()> {
        let tx = {
            let state = self.state.lock().await;
            state.as_ref().map(|state| state.tx.clone())
        };
        match tx {
            Some(tx) => tx.send(Ok(item)).await.map_err(|_| {
                AppError::new(ErrorCode::Internal, "record writer task stopped early")
            }),
            None => Err(AppError::new(
                ErrorCode::Internal,
                "record sink used before prepare",
            )),
        }
    }

    async fn finish(&self) -> AppResult<()> {
        let state = self.state.lock().await.take();
        if let Some(state) = state {
            drop(state.tx);
            state.handle.await.map_err(AppError::internal)??;
        }
        Ok(())
    }
}

impl RecordSink {
    /// Path this sink writes to.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::record::JsonLinesWriter;

    #[tokio::test]
    async fn record_sink_streams_records_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/out.jsonl");
        let sink = RecordSink::new(Arc::new(JsonLinesWriter), &path);

        sink.prepare().await.unwrap();
        sink.write(DatasetRecord::from_fields([("id", json!(1))]))
            .await
            .unwrap();
        sink.write(DatasetRecord::from_fields([("id", json!(2))]))
            .await
            .unwrap();
        sink.finish().await.unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.lines().count(), 2);
        assert!(written.contains("\"id\":1"));
        assert!(written.contains("\"id\":2"));
    }

    #[tokio::test]
    async fn record_sink_write_before_prepare_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let sink = RecordSink::new(Arc::new(JsonLinesWriter), dir.path().join("out.jsonl"));
        let error = sink
            .write(DatasetRecord::from_fields([("id", json!(1))]))
            .await
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Internal);
    }
}

//! Record source adapter — stream a [`DatasetReader`] as a [`Source`] of records.

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::source::{BoxItemStream, Source};

use super::model::DatasetRecord;
use super::reader::DatasetReader;

/// Adapts any [`DatasetReader`] into a [`Source`] the generic engine can collect.
pub struct RecordSource {
    name: String,
    reader: Box<dyn DatasetReader>,
    cache_key: Value,
    max_items: Option<usize>,
}

impl RecordSource {
    /// Create a record source with a stable name over the given reader.
    #[must_use]
    pub fn new(name: impl Into<String>, reader: Box<dyn DatasetReader>) -> Self {
        let name = name.into();
        let cache_key = json!({ "record-source": name });
        Self {
            name,
            reader,
            cache_key,
            max_items: None,
        }
    }

    /// Override the cache key describing this source's configured inputs.
    #[must_use]
    pub fn with_cache_key(mut self, cache_key: Value) -> Self {
        self.cache_key = cache_key;
        self
    }

    /// Declare the maximum number of records this source will emit.
    #[must_use]
    pub fn with_max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }
}

impl Source<DatasetRecord> for RecordSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn stream(self: Box<Self>, _cancel: CancellationToken) -> BoxItemStream<DatasetRecord> {
        self.reader.stream()
    }

    fn cache_key(&self) -> Value {
        self.cache_key.clone()
    }

    fn max_items(&self) -> Option<usize> {
        self.max_items
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt as _;

    use super::*;
    use crate::record::JsonLinesReader;

    #[tokio::test]
    async fn record_source_streams_reader_output_and_exposes_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.jsonl");
        std::fs::write(&path, "{\"id\":1}\n{\"id\":2}\n").unwrap();

        let source = RecordSource::new("records", Box::new(JsonLinesReader::new(&path)))
            .with_max_items(2)
            .with_cache_key(json!({"path": "data.jsonl"}));
        assert_eq!(source.name(), "records");
        assert_eq!(source.max_items(), Some(2));
        assert_eq!(source.cache_key(), json!({"path": "data.jsonl"}));

        let collected = Box::new(source)
            .stream(CancellationToken::new())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(collected.len(), 2);
        assert!(collected.iter().all(Result::is_ok));
    }
}

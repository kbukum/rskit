//! Dataset collection framework — streaming sources, transforms,
//! and schema validation on a generic collection engine.
//!
//! One generic [`Collector<T>`] drives collection for every item family:
//! it is generic over any [`DatasetItem`], so [`DataItem`] blobs
//! and [`DatasetRecord`] rows share the same worker pool, cancellation, and event loop.
//! The engine stays item-agnostic — it never writes items itself.
//! Per-item materialization lives behind an injected [`ItemSink<T>`] ([`LocalBlobSink`] writes [`DataItem`] samples to `real/` and `ai/`),
//! and per-item validation is a pluggable [`Validator<T>`] that callers opt into (for example a schema-backed validator for tabular records).
//! Publishing is the separate [`Target`] concern, which is directory-scoped by design (it publishes the finished output directory, not per item);
//! gokit folds the per-item sink and the directory target into a single item-typed `dataset/stage.Target[T]` (an intentional cross-kit divergence).

#![warn(missing_docs)]

pub mod collector;
mod item;
mod label;
mod limits;
pub mod manifest;
mod media;
mod payload;
pub mod record;
pub mod schema;
mod sink;
pub mod source;
pub mod stream;
pub mod target;
pub mod transform;
mod validate;

pub use collector::{Collector, CollectorConfig, CollectorResult, NullProgress, ProgressCallback};
pub use item::{DataItem, DatasetItem};
pub use label::Label;
pub use limits::{DEFAULT_MAX_IN_MEMORY_BYTES, DatasetLimits};
pub use manifest::{CacheStatus, Manifest, SourceEntry, SourceStats};
pub use media::MediaType;
pub use payload::DataPayload;
pub use record::{
    BoxRecordStream, CsvReader, CsvWriter, DatasetFormat, DatasetReader, DatasetRecord,
    DatasetWriter, JsonArrayReader, JsonArrayWriter, JsonLinesReader, JsonLinesWriter, RecordSink,
    RecordSource, SchemaValidator, filter_records, select_columns,
};
pub use schema::{DatasetSchema, validate_record};
pub use sink::{ItemSink, LocalBlobSink};
pub use source::{BoxDataStream, BoxItemStream, Source};
pub use stream::DatasetStreamExt;
pub use target::{PublishResult, Target};
#[cfg(feature = "image-transform")]
pub use transform::ResizeTransform;
pub use transform::Transform;
pub use validate::Validator;

#[cfg(test)]
mod tests;

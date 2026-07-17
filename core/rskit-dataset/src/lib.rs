//! Dataset collection framework — streaming sources, transforms, targets, and schema validation.

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
pub mod source;
pub mod stream;
pub mod target;
pub mod transform;

pub use collector::{Collector, CollectorConfig, CollectorResult, NullProgress, ProgressCallback};
pub use item::DataItem;
pub use label::Label;
pub use limits::{DEFAULT_MAX_IN_MEMORY_BYTES, DatasetLimits};
pub use manifest::{CacheStatus, Manifest, SourceEntry, SourceStats};
pub use media::MediaType;
pub use payload::DataPayload;
pub use record::{
    BoxRecordStream, CsvReader, CsvWriter, DatasetFormat, DatasetReader, DatasetRecord,
    DatasetWriter, JsonArrayReader, JsonArrayWriter, JsonLinesReader, JsonLinesWriter,
    filter_records, select_columns,
};
pub use schema::{DatasetSchema, validate_record};
pub use source::{BoxDataStream, Source};
pub use stream::DatasetStreamExt;
pub use target::{PublishResult, Target};
#[cfg(feature = "image-transform")]
pub use transform::ResizeTransform;
pub use transform::Transform;

#[cfg(test)]
mod tests;

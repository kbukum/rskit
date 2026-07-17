//! Streaming row/record dataset abstractions.

mod limits;
mod model;
mod ops;
mod reader;
mod sink;
mod source;
mod validator;
mod writer;

pub use model::{DatasetFormat, DatasetRecord};
pub use ops::{BoxRecordStream, filter_records, select_columns};
pub use reader::{CsvReader, DatasetReader, JsonArrayReader, JsonLinesReader};
pub use sink::RecordSink;
pub use source::RecordSource;
pub use validator::SchemaValidator;
pub use writer::{CsvWriter, DatasetWriter, JsonArrayWriter, JsonLinesWriter};

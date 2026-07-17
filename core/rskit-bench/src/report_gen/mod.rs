//! Report generation for bench results.
//!
//! Multiple output formats: Markdown, JSON, CSV, JUnit, Table, VegaLite.

mod csv;
mod json;
mod junit;
mod markdown;
mod reporter;
mod table;
mod vegalite;

pub use self::csv::CsvReporter;
pub use self::json::JsonReporter;
pub use self::junit::JUnitReporter;
pub use self::markdown::MarkdownReporter;
pub use self::reporter::Reporter;
pub use self::table::TableReporter;
pub use self::vegalite::{VegaLiteReporter, vegalite_specs};

pub(crate) use self::reporter::io_err;

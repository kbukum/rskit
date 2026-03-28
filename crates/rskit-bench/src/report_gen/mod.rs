//! Report generation for bench results.
//!
//! Multiple output formats: Markdown, JSON, CSV, JUnit, Table, VegaLite.

mod csv;
mod json;
mod junit;
mod markdown;
mod table;
mod vegalite;

pub use self::csv::CsvReporter;
pub use self::json::JsonReporter;
pub use self::junit::JUnitReporter;
pub use self::markdown::MarkdownReporter;
pub use self::table::TableReporter;
pub use self::vegalite::{VegaLiteReporter, vegalite_specs};

use rskit_errors::AppResult;
use super::result::BenchRunResult;
use std::io::Write;

/// Generates formatted output from benchmark results.
pub trait Reporter: Send + Sync {
    fn name(&self) -> &str;
    fn generate(&self, writer: &mut dyn Write, result: &BenchRunResult) -> AppResult<()>;
}

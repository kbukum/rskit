//! JSON report generation.

use super::Reporter;
use crate::result::BenchRunResult;
use std::io::Write;

/// Generates canonical JSON output with $schema and version.
pub struct JsonReporter;

impl Reporter for JsonReporter {
    fn name(&self) -> &str {
        "json"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let json = serde_json::to_string_pretty(result)?;
        write!(w, "{}", json)?;
        Ok(())
    }
}

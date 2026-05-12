//! JSON report generation.

use super::{Reporter, io_err};
use crate::result::BenchRunResult;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::io::Write;

/// Generates canonical JSON output with $schema and version.
pub struct JsonReporter;

impl Reporter for JsonReporter {
    fn name(&self) -> &str {
        "json"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let json = serde_json::to_string_pretty(result)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("serialize: {e}")))?;
        write!(w, "{json}").map_err(io_err)?;
        Ok(())
    }
}

//! Core reporting contract shared by all bench output formats.

use crate::result::BenchRunResult;
use rskit_errors::AppResult;
use std::io::Write;

/// Generates formatted output from benchmark results.
pub trait Reporter: Send + Sync {
    fn name(&self) -> &str;
    fn generate(&self, writer: &mut dyn Write, result: &BenchRunResult) -> AppResult<()>;
}

pub(crate) fn io_err(e: std::io::Error) -> rskit_errors::AppError {
    rskit_errors::AppError::new(rskit_errors::ErrorCode::Internal, format!("write: {e}"))
}

//! CLI utilities for bench operations.

use crate::compare::RunComparator;
use crate::report_gen::{MarkdownReporter, Reporter};
use crate::result::BenchRunResult;
use crate::run_storage::{FileRunStorage, ListOptions, RunStorage};
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::io::Write;
use std::path::PathBuf;

pub struct CliRunner {
    storage: FileRunStorage,
}

impl CliRunner {
    pub fn new(results_dir: impl Into<PathBuf>) -> Self {
        Self {
            storage: FileRunStorage::new(results_dir),
        }
    }

    pub fn show_latest(&self, writer: &mut dyn Write) -> AppResult<()> {
        let result = self.storage.latest()?;
        self.show_run_detail(writer, &result)
    }

    pub fn show_run(&self, writer: &mut dyn Write, run_id: &str) -> AppResult<()> {
        let result = self.storage.load(run_id)?;
        self.show_run_detail(writer, &result)
    }

    fn show_run_detail(&self, writer: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let reporter = MarkdownReporter;
        reporter.generate(writer, result)
    }

    pub fn compare_runs(
        &self,
        writer: &mut dyn Write,
        base_id: &str,
        target_id: &str,
    ) -> AppResult<()> {
        let base = self.storage.load(base_id)?;
        let target = self.storage.load(target_id)?;
        let comparator = RunComparator::default();
        let diff = comparator.compare(&base, &target);
        writeln!(writer, "{}", diff.summary())
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        Ok(())
    }

    pub fn compare_latest(&self, writer: &mut dyn Write) -> AppResult<()> {
        let runs = self.storage.list(ListOptions::default().with_limit(2))?;
        if runs.len() < 2 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Need at least 2 runs to compare",
            ));
        }
        self.compare_runs(writer, &runs[1].id, &runs[0].id)
    }

    pub fn list_runs(&self, writer: &mut dyn Write, opts: ListOptions) -> AppResult<()> {
        let runs = self.storage.list(opts)?;
        if runs.is_empty() {
            writeln!(writer, "No runs found.")
                .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
            return Ok(());
        }
        writeln!(
            writer,
            "{:<40} {:<24} {:<15} {:>8}",
            "ID", "Timestamp", "Tag", "F1"
        )
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        writeln!(writer, "{}", "-".repeat(90))
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        for r in &runs {
            writeln!(
                writer,
                "{:<40} {:<24} {:<15} {:>8.4}",
                r.id, r.timestamp, r.tag, r.f1
            )
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        }
        writeln!(writer, "\nTotal: {} run(s)", runs.len())
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        Ok(())
    }
}

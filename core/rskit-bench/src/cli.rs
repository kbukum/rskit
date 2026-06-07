//! CLI utilities for bench operations.

use crate::compare::RunComparator;
use crate::report_gen::{MarkdownReporter, Reporter};
use crate::result::BenchRunResult;
use crate::run_storage::{FileRunStorage, ListOptions, RunStorage};
use rskit_cli::OutputTable;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::io::Write;
use std::path::PathBuf;

pub struct CliRunner {
    storage: Box<dyn RunStorage>,
}

impl CliRunner {
    /// Create a CLI runner backed by file storage under `results_dir`.
    pub fn new(results_dir: impl Into<PathBuf>) -> Self {
        Self {
            storage: Box::new(FileRunStorage::new(results_dir)),
        }
    }

    /// Create a CLI runner with injected storage.
    #[must_use]
    pub fn with_storage(storage: Box<dyn RunStorage>) -> Self {
        Self { storage }
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
        let mut table = OutputTable::new(vec![
            "ID".to_string(),
            "Timestamp".to_string(),
            "Tag".to_string(),
            "F1".to_string(),
        ]);
        for r in &runs {
            table.add_row(vec![
                r.id.clone(),
                r.timestamp.clone(),
                r.tag.clone(),
                format!("{:.4}", r.f1),
            ]);
        }
        writeln!(writer, "{table}")
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        writeln!(writer, "\nTotal: {} run(s)", runs.len())
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        Ok(())
    }
}

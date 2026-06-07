//! Storage trait and file-based implementation for bench results.

use crate::result::{BenchRunResult, BenchRunSummary};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::{
    confine_path,
    sync_io::{dir, file},
};
use std::path::{Path, PathBuf};

/// Options for listing stored results.
pub struct ListOptions {
    pub limit: usize,
    pub tag: Option<String>,
    pub dataset: Option<String>,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            tag: None,
            dataset: None,
        }
    }
}

impl ListOptions {
    #[must_use]
    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    #[must_use]
    pub fn with_dataset(mut self, dataset: impl Into<String>) -> Self {
        self.dataset = Some(dataset.into());
        self
    }
}

/// Abstraction for storing/retrieving benchmark results.
pub trait RunStorage: Send + Sync {
    fn save(&self, result: &BenchRunResult) -> AppResult<String>;
    fn load(&self, run_id: &str) -> AppResult<BenchRunResult>;
    fn latest(&self) -> AppResult<BenchRunResult>;
    fn list(&self, opts: ListOptions) -> AppResult<Vec<BenchRunSummary>>;
}

/// File-based storage implementation.
pub struct FileRunStorage {
    dir: PathBuf,
}

impl FileRunStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl RunStorage for FileRunStorage {
    fn save(&self, result: &BenchRunResult) -> AppResult<String> {
        dir::create_all(&self.dir)?;
        let path = run_result_path(&self.dir, &result.id)?;
        let json = serde_json::to_string_pretty(result)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("serialize: {e}")))?;
        file::write(&path, json)?;
        Ok(result.id.clone())
    }

    fn load(&self, run_id: &str) -> AppResult<BenchRunResult> {
        let path = run_result_path(&self.dir, run_id)?;
        let content = file::read_string(&path).map_err(|e| {
            AppError::new(
                ErrorCode::NotFound,
                format!("Run not found: {run_id} ({e})"),
            )
        })?;
        serde_json::from_str(&content)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("deserialize: {e}")))
    }

    fn latest(&self) -> AppResult<BenchRunResult> {
        let summaries = self.list(ListOptions::default().with_limit(1))?;
        if summaries.is_empty() {
            return Err(AppError::new(ErrorCode::NotFound, "No runs found"));
        }
        self.load(&summaries[0].id)
    }

    fn list(&self, opts: ListOptions) -> AppResult<Vec<BenchRunSummary>> {
        if !dir::exists(&self.dir)? {
            return Ok(Vec::new());
        }
        let mut summaries = Vec::new();
        for entry in dir::list(&self.dir)? {
            if !entry.is_file || entry.path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(content) = file::read_string(&entry.path) else {
                tracing::warn!(path = %entry.path.display(), "skipping unreadable bench run result");
                continue;
            };
            let Ok(result) = serde_json::from_str::<BenchRunResult>(&content) else {
                tracing::warn!(path = %entry.path.display(), "skipping invalid bench run result");
                continue;
            };
            if let Some(ref tag) = opts.tag
                && result.tag != *tag
            {
                continue;
            }
            if let Some(ref ds) = opts.dataset
                && result.dataset.name != *ds
            {
                continue;
            }
            let f1 = result
                .metrics
                .iter()
                .find_map(|m| m.values.get("f1").copied())
                .unwrap_or(0.0);
            summaries.push(BenchRunSummary {
                id: result.id,
                timestamp: result.timestamp,
                tag: result.tag,
                dataset: result.dataset.name,
                f1,
            });
        }
        summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if opts.limit > 0 && summaries.len() > opts.limit {
            summaries.truncate(opts.limit);
        }
        Ok(summaries)
    }
}

fn run_result_path(root: &Path, run_id: &str) -> AppResult<PathBuf> {
    if run_id.contains(['/', '\\']) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("run id must not contain path separators: {run_id}"),
        ));
    }
    confine_path(root, Path::new(&format!("{run_id}.json")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::result::{BenchRunResult, DatasetInfo};

    use super::{FileRunStorage, ListOptions, RunStorage};

    fn result(id: &str) -> BenchRunResult {
        BenchRunResult {
            id: id.to_string(),
            schema: crate::schema::schema_url(),
            version: crate::schema::version(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            tag: "main".to_string(),
            duration_ms: 10,
            dataset: DatasetInfo {
                name: "dataset".to_string(),
                version: "1".to_string(),
                sample_count: 1,
                label_distribution: HashMap::new(),
            },
            metrics: Vec::new(),
            branches: HashMap::new(),
            samples: Vec::new(),
            curves: HashMap::new(),
        }
    }

    #[test]
    fn list_skips_invalid_run_result_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = FileRunStorage::new(dir.path());

        storage.save(&result("valid")).expect("save valid result");
        std::fs::write(dir.path().join("partial.json"), "{").expect("write invalid json");

        let summaries = storage
            .list(ListOptions::default())
            .expect("list should skip invalid files");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "valid");
    }

    #[test]
    fn save_rejects_run_ids_with_path_separators() {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = FileRunStorage::new(dir.path());
        let mut result = result("../escaped");

        let error = storage
            .save(&result)
            .expect_err("path separator should fail");

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        result.id = "nested/path".to_string();
        let error = storage.save(&result).expect_err("nested path should fail");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}

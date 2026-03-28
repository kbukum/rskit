//! Storage trait and file-based implementation for bench results.

use crate::result::{BenchRunResult, BenchRunSummary};
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::path::PathBuf;

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
    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

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
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("create dir: {e}")))?;
        let path = self.dir.join(format!("{}.json", result.id));
        let json = serde_json::to_string_pretty(result)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("serialize: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))?;
        Ok(result.id.clone())
    }

    fn load(&self, run_id: &str) -> AppResult<BenchRunResult> {
        let path = self.dir.join(format!("{run_id}.json"));
        let content = std::fs::read_to_string(&path).map_err(|e| {
            AppError::new(
                ErrorCode::NotFound,
                format!("Run not found: {} ({})", run_id, e),
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
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut summaries = Vec::new();
        let entries = std::fs::read_dir(&self.dir)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("read dir: {e}")))?;
        for entry in entries {
            let entry = entry
                .map_err(|e| AppError::new(ErrorCode::Internal, format!("read entry: {e}")))?;
            if entry.path().extension().is_none_or(|e| e != "json") {
                continue;
            }
            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let result: BenchRunResult = match serde_json::from_str(&content) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if let Some(ref tag) = opts.tag {
                if result.tag != *tag {
                    continue;
                }
            }
            if let Some(ref ds) = opts.dataset {
                if result.dataset.name != *ds {
                    continue;
                }
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

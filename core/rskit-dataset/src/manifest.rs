//! `.manifest.json` cache logic — skip re-downloading completed sources.
//!
//! Same format as the Python version for compatibility.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Build manifest persisted to `.manifest.json` in the output directory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub sources: HashMap<String, SourceEntry>,
}

/// Cache entry for a single source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub config: serde_json::Value,
    pub stats: SourceStats,
    pub status: String,
}

/// Statistics for a completed source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceStats {
    pub total: usize,
    pub real: usize,
    pub ai: usize,
    /// API offset reached (for resume). 0 means start from beginning.
    #[serde(default)]
    pub fetched_offset: usize,
}

const MANIFEST_FILE: &str = ".manifest.json";

/// Result of checking whether a source has cached data.
#[derive(Debug, Clone)]
pub enum CacheStatus {
    /// Source fully completed — skip entirely.
    Done(SourceStats),
    /// Source partially completed — resume from where it left off.
    Partial(SourceStats),
    /// No usable cache — fetch from scratch.
    NotCached,
}

impl Manifest {
    /// Load manifest from the output directory. Returns empty manifest if not found.
    pub fn load(output_dir: &Path) -> Self {
        let path = output_dir.join(MANIFEST_FILE);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(manifest) => return manifest,
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to parse manifest, starting fresh");
                    }
                },
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read manifest, starting fresh");
                }
            }
        }
        Self::default()
    }

    /// Save manifest to the output directory.
    pub fn save(&self, output_dir: &Path) -> AppResult<()> {
        let path = output_dir.join(MANIFEST_FILE);
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("manifest serialize failed: {e}"),
            )
        })?;
        std::fs::write(path, content).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("manifest write failed: {e}"))
        })?;
        Ok(())
    }

    /// Check if a source has usable cached data (completed or partial with items).
    pub fn is_cached(&self, source_name: &str, config: &serde_json::Value) -> Option<&SourceStats> {
        let entry = self.sources.get(source_name)?;
        if &entry.config != config {
            return None;
        }
        match entry.status.as_str() {
            "done" => Some(&entry.stats),
            "partial" if entry.stats.total > 0 => Some(&entry.stats),
            _ => None,
        }
    }

    /// Check cache status with distinction between done (skip) and partial (resume).
    pub fn cache_status(
        &self,
        source_name: &str,
        config: &serde_json::Value,
        max_items: Option<usize>,
    ) -> CacheStatus {
        let entry = match self.sources.get(source_name) {
            Some(e) => e,
            None => return CacheStatus::NotCached,
        };
        if &entry.config != config {
            return CacheStatus::NotCached;
        }
        match entry.status.as_str() {
            "done" => CacheStatus::Done(entry.stats.clone()),
            "partial" if entry.stats.total > 0 => {
                // If we're within 1% of max (or ≤5 remaining), consider it done.
                // Scanning 1000s of rows for 1-2 more items is wasteful.
                if let Some(max) = max_items {
                    let remaining = max.saturating_sub(entry.stats.total);
                    if remaining <= 5 || (entry.stats.total * 100 / max.max(1)) >= 99 {
                        return CacheStatus::Done(entry.stats.clone());
                    }
                }
                CacheStatus::Partial(entry.stats.clone())
            }
            _ => CacheStatus::NotCached,
        }
    }

    /// Record a completed source.
    pub fn mark_done(
        &mut self,
        source_name: String,
        config: serde_json::Value,
        stats: SourceStats,
    ) {
        self.sources.insert(
            source_name,
            SourceEntry {
                config,
                stats,
                status: "done".to_string(),
            },
        );
    }

    /// Record a partially completed source (e.g., cancelled).
    pub fn mark_partial(
        &mut self,
        source_name: String,
        config: serde_json::Value,
        stats: SourceStats,
    ) {
        self.sources.insert(
            source_name,
            SourceEntry {
                config,
                stats,
                status: "partial".to_string(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config = serde_json::json!({"repo": "org/dataset", "split": "train"});
        manifest.mark_done(
            "hf:org/dataset".to_string(),
            config.clone(),
            SourceStats {
                total: 1000,
                real: 500,
                ai: 500,
                fetched_offset: 1000,
            },
        );

        manifest.save(dir.path()).unwrap();
        let loaded = Manifest::load(dir.path());

        assert!(loaded.is_cached("hf:org/dataset", &config).is_some());
        let stats = loaded.is_cached("hf:org/dataset", &config).unwrap();
        assert_eq!(stats.total, 1000);
        assert_eq!(stats.real, 500);
    }

    #[test]
    fn test_manifest_config_mismatch() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config1 = serde_json::json!({"repo": "org/dataset", "max_items": 1000});
        manifest.mark_done(
            "source".to_string(),
            config1,
            SourceStats {
                total: 1000,
                real: 500,
                ai: 500,
                fetched_offset: 1000,
            },
        );
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path());
        let config2 = serde_json::json!({"repo": "org/dataset", "max_items": 500});
        assert!(loaded.is_cached("source", &config2).is_none());
    }

    #[test]
    fn test_manifest_empty_dir() {
        let dir = TempDir::new().unwrap();
        let manifest = Manifest::load(dir.path());
        assert!(manifest.sources.is_empty());
    }

    #[test]
    fn test_manifest_partial_with_items_is_cached() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config = serde_json::json!({"repo": "org/dataset", "split": "train"});
        manifest.mark_partial(
            "hf:org/dataset".to_string(),
            config.clone(),
            SourceStats {
                total: 500,
                real: 250,
                ai: 250,
                fetched_offset: 500,
            },
        );
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path());
        let stats = loaded.is_cached("hf:org/dataset", &config);
        assert!(stats.is_some(), "partial with items should be cached");
        assert_eq!(stats.unwrap().total, 500);
    }

    #[test]
    fn test_manifest_partial_with_zero_items_not_cached() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config = serde_json::json!({"repo": "org/dataset"});
        manifest.mark_partial(
            "source".to_string(),
            config.clone(),
            SourceStats {
                total: 0,
                real: 0,
                ai: 0,
                fetched_offset: 0,
            },
        );
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path());
        assert!(loaded.is_cached("source", &config).is_none());
    }

    #[test]
    fn test_manifest_partial_resumes_not_skips() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config = serde_json::json!({"repo": "org/dataset", "split": "train"});
        manifest.mark_partial(
            "hf:org/dataset".to_string(),
            config.clone(),
            SourceStats {
                total: 500,
                real: 250,
                ai: 250,
                fetched_offset: 600,
            },
        );
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path());
        match loaded.cache_status("hf:org/dataset", &config, Some(1000)) {
            CacheStatus::Partial(stats) => {
                assert_eq!(stats.fetched_offset, 600);
                assert_eq!(stats.total, 500);
            }
            other => panic!("expected Partial, got {other:?}"),
        }
    }

    #[test]
    fn test_manifest_done_is_not_partial() {
        let dir = TempDir::new().unwrap();
        let mut manifest = Manifest::default();

        let config = serde_json::json!({"repo": "org/dataset"});
        manifest.mark_done(
            "src".to_string(),
            config.clone(),
            SourceStats {
                total: 1000,
                real: 500,
                ai: 500,
                fetched_offset: 1000,
            },
        );
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path());
        match loaded.cache_status("src", &config, Some(1000)) {
            CacheStatus::Done(stats) => assert_eq!(stats.total, 1000),
            other => panic!("expected Done, got {other:?}"),
        }
    }
}

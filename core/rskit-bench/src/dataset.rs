//! Dataset loading for accuracy benchmarking.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::sync_io::file;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// One sample entry from a benchmark dataset manifest.
pub struct Sample {
    /// Stable identifier used in reports and result comparisons.
    pub id: String,
    /// Path to the sample content file relative to the dataset directory.
    pub file: String,
    /// Ground-truth label as it appears in the manifest.
    pub label: String,
    #[serde(default)]
    /// Optional source reference for the sample.
    pub source: String,
    #[serde(default)]
    /// Optional human-readable sample description.
    pub description: String,
    #[serde(default)]
    /// Additional manifest metadata attached to the sample.
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Manifest describing the samples in a benchmark dataset.
pub struct DatasetManifest {
    /// Dataset name reported with benchmark results.
    pub name: String,
    #[serde(default = "default_version")]
    /// Dataset version string reported with benchmark results.
    pub version: String,
    /// Samples available in the dataset.
    pub samples: Vec<Sample>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Loads `manifest.json` from a dataset directory and validates that referenced sample files exist.
pub fn load_manifest(dir: &Path) -> AppResult<DatasetManifest> {
    load_manifest_file(dir, "manifest.json")
}

pub(crate) fn load_manifest_file(dir: &Path, manifest_file: &str) -> AppResult<DatasetManifest> {
    let manifest_path = dir.join(manifest_file);
    let content = file::read_string(&manifest_path).map_err(|e| {
        AppError::new(
            ErrorCode::NotFound,
            format!(
                "Failed to read manifest at {}: {}",
                manifest_path.display(),
                e
            ),
        )
    })?;

    let manifest: DatasetManifest = serde_json::from_str(&content)
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("parse manifest: {e}")))?;

    for sample in &manifest.samples {
        let sample_path = dir.join(&sample.file);
        if !sample_path.exists() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!(
                    "Sample file not found: {} (id={})",
                    sample_path.display(),
                    sample.id
                ),
            ));
        }
    }

    Ok(manifest)
}

/// Reads the content bytes for `sample` from the dataset directory.
pub fn load_content(dir: &Path, sample: &Sample) -> AppResult<Vec<u8>> {
    let path = dir.join(&sample.file);
    file::read(&path).map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("Failed to read sample {}: {}", path.display(), e),
        )
    })
}

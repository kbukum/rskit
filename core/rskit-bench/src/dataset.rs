//! Dataset loading for accuracy benchmarking.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::sync_io::file;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub id: String,
    pub file: String,
    pub label: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub samples: Vec<Sample>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

pub fn load_manifest(dir: &Path) -> AppResult<DatasetManifest> {
    let manifest_path = dir.join("manifest.json");
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

pub fn load_content(dir: &Path, sample: &Sample) -> AppResult<Vec<u8>> {
    let path = dir.join(&sample.file);
    file::read(&path).map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("Failed to read sample {}: {}", path.display(), e),
        )
    })
}

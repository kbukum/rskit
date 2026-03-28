//! Target trait — publish collected data to a destination.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    pub target_name: String,
    pub location: String,
    pub files_published: usize,
    pub message: String,
}

#[async_trait::async_trait]
pub trait Target: Send + Sync {
    fn name(&self) -> &str;

    async fn publish(
        &self,
        directory: &Path,
        metadata: Option<&std::collections::HashMap<String, String>>,
    ) -> AppResult<PublishResult>;
}

/// Local filesystem target — data is already on disk.
pub struct LocalTarget;

#[async_trait::async_trait]
impl Target for LocalTarget {
    fn name(&self) -> &str {
        "local"
    }

    async fn publish(
        &self,
        directory: &Path,
        _metadata: Option<&std::collections::HashMap<String, String>>,
    ) -> AppResult<PublishResult> {
        let mut file_count = 0usize;
        if directory.exists() {
            for entry in walkdir(directory)? {
                if entry.is_file() {
                    file_count += 1;
                }
            }
        }
        Ok(PublishResult {
            target_name: self.name().to_string(),
            location: directory.display().to_string(),
            files_published: file_count,
            message: format!("Data saved to {}", directory.display()),
        })
    }
}

fn walkdir(dir: &Path) -> AppResult<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("read dir failed: {e}")))?
        {
            let entry = entry.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("dir entry error: {e}"))
            })?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path)?);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

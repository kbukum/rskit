//! Target trait — publish collected data to a destination.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result returned by a publish target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    /// Target identifier.
    pub target_name: String,
    /// Published location.
    pub location: String,
    /// Number of files published or observed by the target.
    pub files_published: usize,
    /// Human-readable publish summary.
    pub message: String,
}

/// Destination for collected dataset output.
#[async_trait::async_trait]
pub trait Target: Send + Sync {
    /// Stable target name.
    fn name(&self) -> &str;

    /// Publish the dataset directory and optional metadata.
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
        let directory = directory.to_path_buf();
        let file_count = tokio::task::spawn_blocking({
            let directory = directory.clone();
            move || {
                let mut file_count = 0usize;
                if directory.exists() {
                    for entry in walkdir(&directory)? {
                        if entry.is_file() {
                            file_count += 1;
                        }
                    }
                }
                Ok::<usize, AppError>(file_count)
            }
        })
        .await
        .map_err(AppError::internal)??;
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
            let entry = entry
                .map_err(|e| AppError::new(ErrorCode::Internal, format!("dir entry error: {e}")))?;
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

//! Target trait — publish a collected dataset *directory* to a destination.
//!
//! [`Target`] is **directory-scoped by design**: it publishes the finished output directory the collector materialized, so — deliberately unlike the item-typed [`Source`](crate::Source) / [`Transform`](crate::transform::Transform) / [`Validator`](crate::validate::Validator) — it is **not** generic over the item type. Per-item materialization is a separate concern owned by the generic [`ItemSink<T>`](crate::ItemSink), which the engine streams each item into *before* any target publishes the completed directory.
//!
//! Cross-kit mapping: gokit makes the opposite-but-equivalent decision, folding the per-item sink and the directory target into a single item-typed `dataset/stage.Target[T]` (consumes a `stream.Pipeline[T]`). So rskit's `ItemSink<T>` + directory `Target` split maps onto gokit's one `stage.Target[T]`; both kits cover the same publish concern, and the divergence is intentional (see the gokit parity matrix at <https://github.com/kbukum/gokit/blob/main/docs/PARITY-MATRIX.md>, dataset row).

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
///
/// `Target` is **directory-scoped by design**: `publish` receives the finished output *directory* the collector produced, not individual items — so it is deliberately non-generic, unlike the item-typed [`Source`](crate::Source), [`Transform`](crate::transform::Transform), and [`Validator`](crate::validate::Validator). Per-item materialization is the separate, generic [`ItemSink<T>`](crate::ItemSink) concern, run to completion before any `Target` publishes. (gokit folds both into one item-typed `stage.Target[T]`; see the module docs for the mapping.)
#[async_trait::async_trait]
pub trait Target: Send + Sync {
    /// Stable target name.
    fn name(&self) -> &str;

    /// Publish the produced dataset directory and optional metadata.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_target_counts_nested_files_and_missing_directory_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("nested").join("b.txt"), b"b").unwrap();

        let target = LocalTarget;
        let result = target.publish(dir.path(), None).await.unwrap();

        assert_eq!(target.name(), "local");
        assert_eq!(result.target_name, "local");
        assert_eq!(result.files_published, 2);
        assert!(result.message.contains("Data saved"));

        let missing = dir.path().join("missing");
        let result = target.publish(&missing, None).await.unwrap();
        assert_eq!(result.files_published, 0);
    }

    #[test]
    fn walkdir_rejects_file_as_directory_with_empty_listing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(walkdir(&file).unwrap().is_empty());
    }
}

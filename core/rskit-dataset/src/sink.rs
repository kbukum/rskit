//! Per-item materialization seam — where the generic engine hands each collected item to storage.
//!
//! The engine counts and routes items via [`DatasetItem`], but never writes them itself. A
//! [`ItemSink`] owns all item-specific materialization: [`LocalBlobSink`] writes [`DataItem`]
//! samples to `real/` and `ai/` directories with offset filenames, exactly as the collector did
//! before the engine was generalized.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{DataItem, DatasetItem, DatasetLimits, Label};

/// Destination that materializes each collected item as the engine streams it.
#[async_trait::async_trait]
pub trait ItemSink<T: DatasetItem>: Send + Sync {
    /// Stable sink name.
    fn name(&self) -> &str;

    /// Prepare the destination before any item is written (create directories, seed counters).
    async fn prepare(&self) -> AppResult<()> {
        Ok(())
    }

    /// Materialize a single collected item.
    async fn write(&self, item: T) -> AppResult<()>;

    /// Flush and finalize the destination after the last item is written.
    async fn finish(&self) -> AppResult<()> {
        Ok(())
    }
}

/// Local filesystem sink for [`DataItem`] samples.
///
/// Writes real samples to `<output>/real` and AI-generated samples to `<output>/ai`, naming files
/// by a shared, monotonically increasing counter that resumes past any files already on disk.
pub struct LocalBlobSink {
    real_dir: PathBuf,
    ai_dir: PathBuf,
    limits: DatasetLimits,
    file_counter: AtomicUsize,
}

impl LocalBlobSink {
    /// Create a sink rooted at `output_dir`, writing into its `real/` and `ai/` subdirectories.
    #[must_use]
    pub fn new(output_dir: &Path, limits: DatasetLimits) -> Self {
        Self {
            real_dir: output_dir.join("real"),
            ai_dir: output_dir.join("ai"),
            limits,
            file_counter: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl ItemSink<DataItem> for LocalBlobSink {
    fn name(&self) -> &str {
        "local-blob"
    }

    async fn prepare(&self) -> AppResult<()> {
        let real_dir = self.real_dir.clone();
        let ai_dir = self.ai_dir.clone();
        let existing = tokio::task::spawn_blocking(move || -> AppResult<usize> {
            create_dir(&real_dir)?;
            create_dir(&ai_dir)?;
            Ok(count_files(&real_dir)? + count_files(&ai_dir)?)
        })
        .await
        .map_err(AppError::internal)??;
        self.file_counter.store(existing, Ordering::SeqCst);
        Ok(())
    }

    async fn write(&self, item: DataItem) -> AppResult<()> {
        let subdir = if item.label() == Label::Real {
            &self.real_dir
        } else {
            &self.ai_dir
        };
        let file_idx = self.file_counter.fetch_add(1, Ordering::SeqCst);
        let path = subdir.join(format!("{:06}{}", file_idx, item.extension));
        let limits = self.limits;
        tokio::task::spawn_blocking(move || item.write_to_path(&path, &limits))
            .await
            .map_err(AppError::internal)??;
        Ok(())
    }
}

fn create_dir(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "failed to create dataset directory {}: {error}",
                dir.display()
            ),
        )
    })
}

/// Count regular files directly under `dir`, treating a missing directory as empty.
pub(crate) fn count_files(dir: &Path) -> AppResult<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    std::fs::read_dir(dir)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "failed to read dataset output directory {}: {error}",
                    dir.display()
                ),
            )
        })
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .count()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaType;

    #[test]
    fn count_files_ignores_directories_and_reports_read_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        assert_eq!(count_files(dir.path()).unwrap(), 1);
        assert_eq!(count_files(&dir.path().join("missing")).unwrap(), 0);

        let file = dir.path().join("not-dir");
        std::fs::write(&file, b"x").unwrap();
        let err = count_files(&file).unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(
            err.to_string()
                .contains("failed to read dataset output directory")
        );
    }

    #[tokio::test]
    async fn local_blob_sink_prepares_dirs_and_writes_labeled_files() {
        let out = tempfile::tempdir().unwrap();
        let sink = LocalBlobSink::new(out.path(), DatasetLimits::default());
        sink.prepare().await.unwrap();

        let real = DataItem::new(b"r".to_vec(), Label::Real, MediaType::Text, "s")
            .unwrap()
            .with_extension(".txt");
        let ai = DataItem::new(b"a".to_vec(), Label::AiGenerated, MediaType::Text, "s")
            .unwrap()
            .with_extension(".txt");
        sink.write(real).await.unwrap();
        sink.write(ai).await.unwrap();

        assert_eq!(
            std::fs::read(out.path().join("real/000000.txt")).unwrap(),
            b"r"
        );
        assert_eq!(
            std::fs::read(out.path().join("ai/000001.txt")).unwrap(),
            b"a"
        );
    }

    #[tokio::test]
    async fn local_blob_sink_prepare_reports_directory_creation_errors() {
        let out = tempfile::tempdir().unwrap();
        let blocker = out.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let sink = LocalBlobSink::new(&blocker, DatasetLimits::default());
        assert_eq!(
            sink.prepare().await.unwrap_err().code(),
            ErrorCode::Internal
        );
    }

    #[tokio::test]
    async fn local_blob_sink_counter_resumes_past_existing_files() {
        let out = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(out.path().join("real")).unwrap();
        std::fs::write(out.path().join("real/000000.txt"), b"old").unwrap();

        let sink = LocalBlobSink::new(out.path(), DatasetLimits::default());
        sink.prepare().await.unwrap();
        let item = DataItem::new(b"new".to_vec(), Label::Real, MediaType::Text, "s")
            .unwrap()
            .with_extension(".txt");
        sink.write(item).await.unwrap();
        assert!(out.path().join("real/000001.txt").exists());
    }
}

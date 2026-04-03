//! Streaming I/O helpers for copying and transferring files.

use rskit_errors::AppResult;

use crate::{FileSink, FileSource, ProgressCallback, store::FileStore, store::StoredFile};

/// Copy a file from source to sink, optionally reporting progress.
pub async fn copy_file(
    source: &FileSource,
    sink: &FileSink,
    _on_progress: Option<ProgressCallback>,
) -> AppResult<FileSource> {
    let data = source.read_all().await?;
    let mut writer = sink.writer().await?;
    writer.write_all(&data).await?;
    writer.finalize().await
}

/// Transfer a file between two stores.
pub async fn transfer(
    from_store: &dyn FileStore,
    from_key: &str,
    to_store: &dyn FileStore,
    to_key: &str,
) -> AppResult<StoredFile> {
    let source = from_store.download(from_key).await?;
    let meta = from_store.head(from_key).await?;
    to_store
        .upload(&source, to_key, Some(&meta.content_type), None)
        .await
}

//! Streaming I/O helpers for copying and transferring files.

use rskit_errors::AppResult;

use crate::{
    FileSink, FileSource, ProgressCallback, UploadOptions, store::FileStore, store::StoredFile,
};

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
        .upload(
            &source,
            to_key,
            UploadOptions::new().with_content_type(&meta.content_type),
        )
        .await
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::{FileStore, LocalStore, LocalStoreConfig};

    #[tokio::test]
    async fn copy_file_moves_source_bytes_to_each_sink_type() {
        let source = FileSource::from_bytes(Bytes::from_static(b"copy-data"));

        let memory = copy_file(&source, &FileSink::Memory, None).await.unwrap();
        assert_eq!(
            memory.read_all().await.unwrap(),
            Bytes::from_static(b"copy-data")
        );

        let temp = copy_file(&source, &FileSink::Temp, None).await.unwrap();
        assert_eq!(
            temp.read_all().await.unwrap(),
            Bytes::from_static(b"copy-data")
        );

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("copied.bin");
        let path = copy_file(&source, &FileSink::Path(output.clone()), None)
            .await
            .unwrap();
        assert!(matches!(path, FileSource::Path(_)));
        assert_eq!(tokio::fs::read(output).await.unwrap(), b"copy-data");
    }

    #[tokio::test]
    async fn transfer_downloads_from_source_store_and_uploads_to_destination() {
        let from_dir = tempfile::tempdir().unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        let from_store = LocalStore::new(LocalStoreConfig {
            root_dir: from_dir.path().to_path_buf(),
            auto_create: false,
        })
        .unwrap();
        let to_store = LocalStore::new(LocalStoreConfig {
            root_dir: to_dir.path().to_path_buf(),
            auto_create: false,
        })
        .unwrap();

        from_store
            .upload(
                &FileSource::from_bytes(Bytes::from_static(b"transfer-data")),
                "source.bin",
                UploadOptions::new().with_content_type("application/custom"),
            )
            .await
            .unwrap();

        let stored = transfer(&from_store, "source.bin", &to_store, "nested/target.bin")
            .await
            .unwrap();

        assert_eq!(stored.key, "nested/target.bin");
        assert_eq!(stored.content_type, "application/octet-stream");
        assert_eq!(
            to_store
                .download("nested/target.bin")
                .await
                .unwrap()
                .read_all()
                .await
                .unwrap(),
            Bytes::from_static(b"transfer-data")
        );
    }
}

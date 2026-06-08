//! Extended tests for rskit-storage: FileSource, FileSink, TempFile/TempDir, meta, LocalStore.
//!
//! Covers: read/write operations, metadata detection, temp file lifecycle,
//! error handling, copy operations, large files, concurrent access, config validation.

use std::collections::HashMap;

use bytes::Bytes;
use rskit_errors::{AppError, ErrorCode};
use rskit_storage::*;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn make_store(dir: &std::path::Path) -> store::LocalStore {
    store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.to_path_buf(),
        auto_create: true,
    })
    .expect("create local store")
}

// ── FileSource read operations ─────────────────────────────────────────────

#[tokio::test]
async fn source_bytes_read_all_returns_exact_content() {
    let source = FileSource::from_bytes(Bytes::from_static(b"precise content"));
    let data = source.read_all().await.unwrap();
    assert_eq!(data.as_ref(), b"precise content");
}

#[tokio::test]
async fn source_bytes_empty_read_all() {
    let source = FileSource::from_bytes(Bytes::new());
    let data = source.read_all().await.unwrap();
    assert!(data.is_empty());
}

#[tokio::test]
async fn source_path_nonexistent_read_all_fails() {
    let source = FileSource::from_path("/nonexistent/path/file.txt");
    let result = source.read_all().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn source_path_read_via_reader() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"reader test").unwrap();
    let source = FileSource::from_path(tmp.path());

    let mut reader = source.reader().await.unwrap();
    let mut buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf)
        .await
        .unwrap();
    assert_eq!(buf, b"reader test");
}

#[tokio::test]
async fn source_bytes_reader() {
    let source = FileSource::from_bytes(Bytes::from_static(b"bytes reader"));
    let mut reader = source.reader().await.unwrap();
    let mut buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf)
        .await
        .unwrap();
    assert_eq!(buf, b"bytes reader");
}

#[tokio::test]
async fn source_temp_reader() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"temp reader").unwrap();
    let source = FileSource::Temp(tmp);

    let mut reader = source.reader().await.unwrap();
    let mut buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf)
        .await
        .unwrap();
    assert_eq!(buf, b"temp reader");
}

#[tokio::test]
async fn source_url_reader_returns_error() {
    let source = FileSource::from_url("https://example.com/file.txt");
    let result = source.reader().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn source_url_to_local_path_returns_error() {
    let source = FileSource::from_url("https://example.com/file.bin");
    let result = source.to_local_path().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn source_size_url_returns_none() {
    let source = FileSource::from_url("https://example.com/file.txt");
    let size = source.size().await.unwrap();
    assert_eq!(size, None);
}

#[tokio::test]
async fn source_size_temp() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"five5").unwrap();
    let source = FileSource::Temp(tmp);
    let size = source.size().await.unwrap();
    assert_eq!(size, Some(5));
}

#[tokio::test]
async fn source_size_nonexistent_path_fails() {
    let source = FileSource::from_path("/nonexistent/file.txt");
    let result = source.size().await;
    assert!(result.is_err());
}

// ── FileSource extensions ──────────────────────────────────────────────────

#[test]
fn source_extension_from_url() {
    let source = FileSource::from_url("https://example.com/path/file.mp4?token=abc");
    assert_eq!(source.extension(), Some("mp4"));
}

#[test]
fn source_extension_from_url_no_ext() {
    let source = FileSource::from_url("https://example.com/path/file");
    // "file" from rsplit('.') gives "com/path/file" which is > 10 chars, so None
    assert!(source.extension().is_none());
}

#[test]
fn source_extension_from_bytes_is_none() {
    let source = FileSource::from_bytes(Bytes::from_static(b"data"));
    assert_eq!(source.extension(), None);
}

#[test]
fn source_extension_from_temp() {
    let tmp = TempFile::with_extension("wav").unwrap();
    let source = FileSource::Temp(tmp);
    assert_eq!(source.extension(), Some("wav"));
}

// ── FileSource to_local_path ───────────────────────────────────────────────

#[tokio::test]
async fn source_to_local_path_from_path() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"local").unwrap();
    let source = FileSource::from_path(tmp.path());
    let resolved = source.to_local_path().await.unwrap();
    assert_eq!(resolved.path(), tmp.path());
}

#[tokio::test]
async fn source_to_local_path_from_bytes_creates_temp() {
    let source = FileSource::from_bytes(Bytes::from_static(b"from bytes"));
    let resolved = source.to_local_path().await.unwrap();
    let content = std::fs::read(resolved.path()).unwrap();
    assert_eq!(content, b"from bytes");
}

#[tokio::test]
async fn source_to_local_path_from_bytes_keeps_temp_alive_until_resolved_path_drops() {
    let source = FileSource::from_bytes(Bytes::from_static(b"scoped temp"));
    let resolved = source.to_local_path().await.unwrap();
    let path = resolved.path().to_path_buf();

    assert_eq!(std::fs::read(&path).unwrap(), b"scoped temp");

    drop(resolved);

    assert!(!path.exists());
}

#[tokio::test]
async fn source_to_local_path_from_temp() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"temp local").unwrap();
    let source = FileSource::Temp(tmp);
    let resolved = source.to_local_path().await.unwrap();
    let content = std::fs::read(resolved.path()).unwrap();
    assert_eq!(content, b"temp local");
}

// ── FileSink write operations ──────────────────────────────────────────────

#[tokio::test]
async fn sink_path_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("a/b/c/output.txt");
    let sink = FileSink::Path(out.clone());
    let mut writer = sink.writer().await.unwrap();
    writer.write_all(b"nested").await.unwrap();
    let result = writer.finalize().await.unwrap();
    match &result {
        FileSource::Path(p) => assert_eq!(p, &out),
        _ => panic!("expected Path"),
    }
    let content = std::fs::read_to_string(&out).unwrap();
    assert_eq!(content, "nested");
}

#[tokio::test]
async fn sink_memory_empty_write() {
    let sink = FileSink::Memory;
    let mut writer = sink.writer().await.unwrap();
    writer.write_all(b"").await.unwrap();
    let result = writer.finalize().await.unwrap();
    match result {
        FileSource::Bytes(b) => assert!(b.is_empty()),
        _ => panic!("expected Bytes"),
    }
}

#[tokio::test]
async fn sink_memory_multiple_writes() {
    let sink = FileSink::Memory;
    let mut writer = sink.writer().await.unwrap();
    writer.write_all(b"hello ").await.unwrap();
    writer.write_all(b"world").await.unwrap();
    let result = writer.finalize().await.unwrap();
    match result {
        FileSource::Bytes(b) => assert_eq!(b.as_ref(), b"hello world"),
        _ => panic!("expected Bytes"),
    }
}

#[tokio::test]
async fn sink_write_stream_returns_chunk_error_without_finalizing_partial_output() {
    let sink = FileSink::Memory;
    let mut writer = sink.writer().await.unwrap();
    let stream = futures::stream::iter([
        Ok(Bytes::from_static(b"before error")),
        Err(AppError::new(
            ErrorCode::Internal,
            "synthetic stream failure",
        )),
    ]);

    let error = writer.write_stream(stream).await.unwrap_err();

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(error.to_string().contains("synthetic stream failure"));
}

#[tokio::test]
async fn sink_path_write_large_data() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("large.bin");
    let data = vec![0xABu8; 1024 * 1024]; // 1MB

    let sink = FileSink::Path(out.clone());
    let mut writer = sink.writer().await.unwrap();
    writer.write_all(&data).await.unwrap();
    writer.finalize().await.unwrap();

    let content = std::fs::read(&out).unwrap();
    assert_eq!(content.len(), data.len());
    assert_eq!(content, data);
}

// ── FileSink Debug ─────────────────────────────────────────────────────────

#[test]
fn sink_debug_formatting() {
    let sink_mem = FileSink::Memory;
    assert_eq!(format!("{:?}", sink_mem), "Memory");
    let sink_temp = FileSink::Temp;
    assert_eq!(format!("{:?}", sink_temp), "Temp");
    let sink_path = FileSink::Path(std::path::PathBuf::from("/test/path"));
    let dbg = format!("{:?}", sink_path);
    assert!(dbg.contains("Path"));
}

// ── Metadata detection ─────────────────────────────────────────────────────

#[tokio::test]
async fn detect_mime_from_bytes_png() {
    // Minimal PNG header bytes
    let png_header: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
    ];
    let source = FileSource::from_bytes(Bytes::from(png_header));
    let mime = detect_mime(&source).await.unwrap();
    assert!(mime.contains("png"), "expected png, got: {}", mime);
}

#[tokio::test]
async fn detect_mime_unknown_bytes_fallback() {
    let source = FileSource::from_bytes(Bytes::from_static(b"random unknown data here"));
    let mime = detect_mime(&source).await.unwrap();
    assert_eq!(mime, "application/octet-stream");
}

#[tokio::test]
async fn detect_mime_from_extension_txt() {
    let tmp = TempFile::with_extension("txt").unwrap();
    std::fs::write(tmp.path(), b"plain text").unwrap();
    let source = FileSource::from_path(tmp.path());
    let mime = detect_mime(&source).await.unwrap();
    assert!(
        mime.contains("text") || mime == "application/octet-stream",
        "got: {}",
        mime
    );
}

#[tokio::test]
async fn detect_kind_from_mime_categories() {
    assert_eq!(FileKind::from_mime("video/mp4"), FileKind::Video);
    assert_eq!(FileKind::from_mime("audio/mpeg"), FileKind::Audio);
    assert_eq!(FileKind::from_mime("image/jpeg"), FileKind::Image);
    assert_eq!(FileKind::from_mime("text/plain"), FileKind::Text);
    assert_eq!(FileKind::from_mime("application/pdf"), FileKind::Document);
    assert_eq!(FileKind::from_mime("application/zip"), FileKind::Archive);
    assert_eq!(FileKind::from_mime("application/json"), FileKind::Text);
    assert_eq!(
        FileKind::from_mime("application/octet-stream"),
        FileKind::Binary
    );
    assert_eq!(FileKind::from_mime("something/weird"), FileKind::Unknown);
}

#[tokio::test]
async fn detect_kind_document_variants() {
    assert_eq!(
        FileKind::from_mime("application/msword"),
        FileKind::Document
    );
    assert_eq!(
        FileKind::from_mime("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        FileKind::Document
    );
    assert_eq!(
        FileKind::from_mime(
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        ),
        FileKind::Document
    );
}

#[tokio::test]
async fn detect_kind_archive_variants() {
    assert_eq!(FileKind::from_mime("application/x-tar"), FileKind::Archive);
    assert_eq!(FileKind::from_mime("application/gzip"), FileKind::Archive);
    assert_eq!(
        FileKind::from_mime("application/x-bzip2"),
        FileKind::Archive
    );
    assert_eq!(FileKind::from_mime("application/x-rar"), FileKind::Archive);
    assert_eq!(
        FileKind::from_mime("application/x-7z-compressed"),
        FileKind::Archive
    );
    assert_eq!(FileKind::from_mime("application/x-xz"), FileKind::Archive);
}

#[tokio::test]
async fn detect_kind_text_application_variants() {
    assert_eq!(FileKind::from_mime("application/xml"), FileKind::Text);
    assert_eq!(FileKind::from_mime("application/x-yaml"), FileKind::Text);
    assert_eq!(
        FileKind::from_mime("application/javascript"),
        FileKind::Text
    );
}

#[tokio::test]
async fn file_meta_from_bytes() {
    let source = FileSource::from_bytes(Bytes::from_static(b"meta test bytes"));
    let meta = file_meta(&source).await.unwrap();
    assert_eq!(meta.size, Some(15));
    assert!(meta.name.is_none());
    assert!(meta.extension.is_none());
}

#[tokio::test]
async fn file_meta_from_path_has_name_and_ext() {
    let tmp = TempFile::with_extension("json").unwrap();
    std::fs::write(tmp.path(), b"{}").unwrap();
    let source = FileSource::from_path(tmp.path());
    let meta = file_meta(&source).await.unwrap();
    assert!(meta.name.is_some());
    assert_eq!(meta.extension, Some("json".to_string()));
    assert_eq!(meta.size, Some(2));
}

#[tokio::test]
async fn file_meta_from_url() {
    let source = FileSource::from_url("https://example.com/path/video.mp4?q=1");
    // file_meta will fail on read for URL, but we can test the URL source construction
    assert_eq!(source.extension(), Some("mp4"));
}

#[tokio::test]
async fn file_meta_from_temp() {
    let tmp = TempFile::with_extension("csv").unwrap();
    std::fs::write(tmp.path(), b"a,b,c").unwrap();
    let source = FileSource::Temp(tmp);
    let meta = file_meta(&source).await.unwrap();
    assert_eq!(meta.extension, Some("csv".to_string()));
    assert_eq!(meta.size, Some(5));
}

// ── TempFile creation/cleanup ──────────────────────────────────────────────

#[test]
fn temp_file_new_exists_on_disk() {
    let tmp = TempFile::new().unwrap();
    assert!(tmp.path().exists());
}

#[test]
fn temp_file_in_dir() {
    let dir = TempDir::new().unwrap();
    let tmp = TempFile::in_dir(dir.path()).unwrap();
    assert!(tmp.path().starts_with(dir.path()));
    assert!(tmp.path().exists());
}

#[test]
fn temp_file_in_dir_with_extension() {
    let dir = TempDir::new().unwrap();
    let tmp = TempFile::in_dir_with_extension(dir.path(), "mp3").unwrap();
    assert!(tmp.path().to_string_lossy().ends_with(".mp3"));
    assert!(tmp.path().starts_with(dir.path()));
}

#[test]
fn temp_file_clone_creates_independent_copy() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"original").unwrap();
    let cloned = tmp.try_clone().unwrap();
    // Both should exist
    assert!(tmp.path().exists());
    assert!(cloned.path().exists());
    // Paths should differ
    assert_ne!(tmp.path(), cloned.path());
    // Cloned should have the same content
    let content = std::fs::read(cloned.path()).unwrap();
    assert_eq!(content, b"original");
}

#[test]
fn temp_file_persist_removes_original() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"persist data").unwrap();
    let original_path = tmp.path().to_path_buf();

    let target_dir = tempfile::tempdir().unwrap();
    let target = target_dir.path().join("persisted.dat");
    let result = tmp.persist(&target).unwrap();

    assert_eq!(result, target);
    assert!(target.exists());
    assert!(!original_path.exists());
}

#[test]
fn temp_dir_create_file_with_extension() {
    let dir = TempDir::new().unwrap();
    let f = dir.create_file_with_extension("log").unwrap();
    assert!(f.path().to_string_lossy().ends_with(".log"));
    assert!(f.path().starts_with(dir.path()));
}

#[test]
fn temp_dir_cleanup_removes_created_files_on_drop() {
    let dir_path = {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("owned.txt"), b"cleanup").unwrap();
        dir.path().to_path_buf()
    };

    assert!(!dir_path.exists());
}

#[test]
fn temp_file_in_dir_errors_for_missing_directory() {
    let dir = TempDir::new().unwrap();
    let missing_dir = dir.path().join("missing");

    let error = TempFile::in_dir(&missing_dir).unwrap_err();

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(error.to_string().contains("failed to create temp file"));
}

#[test]
fn temp_dir_debug_format() {
    let dir = TempDir::new().unwrap();
    let dbg = format!("{:?}", dir);
    assert!(dbg.contains("TempDir"));
    assert!(dbg.contains("path"));
}

#[test]
fn resolved_path_debug_format() {
    // We can't easily construct ResolvedPath directly, so test via to_local_path
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let source = FileSource::from_bytes(Bytes::from_static(b"debug"));
        let resolved = source.to_local_path().await.unwrap();
        let dbg = format!("{:?}", resolved);
        assert!(dbg.contains("ResolvedPath"));
    });
}

#[test]
fn resolved_path_as_ref() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let tmp = TempFile::new().unwrap();
        std::fs::write(tmp.path(), b"asref").unwrap();
        let source = FileSource::from_path(tmp.path());
        let resolved = source.to_local_path().await.unwrap();
        let path: &std::path::Path = resolved.as_ref();
        assert!(path.exists());
    });
}

// ── Error handling ─────────────────────────────────────────────────────────

#[tokio::test]
async fn store_download_nonexistent_fails() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let result = store.download("missing.txt").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn store_delete_nonexistent_fails() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let error = store.delete("missing.txt").await.unwrap_err();
    assert_eq!(error.code(), rskit_errors::ErrorCode::NotFound);
}

#[tokio::test]
async fn store_head_nonexistent_fails() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let result = store.head("missing.txt").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn store_copy_nonexistent_source_fails() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let result = store.copy("nope.txt", "dest.txt").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn store_rename_nonexistent_source_fails() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let result = store.rename("nope.txt", "dest.txt").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn store_new_without_auto_create_nonexistent_dir_fails() {
    let result = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: std::path::PathBuf::from("/nonexistent/store/root"),
        auto_create: false,
    });
    assert!(result.is_err());
}

// ── LocalStore operations ──────────────────────────────────────────────────

#[tokio::test]
async fn store_upload_with_metadata() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let mut meta = HashMap::new();
    meta.insert("author".to_string(), "test".to_string());

    let source = FileSource::from_bytes(Bytes::from_static(b"with meta"));
    let stored = store
        .upload(&source, "meta.txt", Some("text/plain"), Some(meta.clone()))
        .await
        .unwrap();
    assert_eq!(stored.key, "meta.txt");
    assert_eq!(stored.content_type, "text/plain");
    assert_eq!(stored.metadata.get("author").unwrap(), "test");
}

#[tokio::test]
async fn store_upload_default_content_type() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"data"));
    let stored = store.upload(&source, "file.bin", None, None).await.unwrap();
    assert_eq!(stored.content_type, "application/octet-stream");
}

#[tokio::test]
async fn store_head_returns_correct_metadata() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let data = Bytes::from_static(b"head test content");
    let source = FileSource::from_bytes(data.clone());
    store
        .upload(&source, "head.txt", Some("text/plain"), None)
        .await
        .unwrap();

    let info = store.head("head.txt").await.unwrap();
    assert_eq!(info.key, "head.txt");
    assert_eq!(info.size, data.len() as u64);
}

#[tokio::test]
async fn store_head_rejects_directory() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    std::fs::create_dir(dir.path().join("nested")).unwrap();

    let error = store.head("nested").await.unwrap_err();
    assert_eq!(error.code(), rskit_errors::ErrorCode::NotFound);
}

#[cfg(unix)]
#[tokio::test]
async fn store_head_rejects_symlink() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    std::fs::write(dir.path().join("target.txt"), b"target").unwrap();
    std::os::unix::fs::symlink(dir.path().join("target.txt"), dir.path().join("link.txt")).unwrap();

    let error = store.head("link.txt").await.unwrap_err();
    assert_eq!(error.code(), rskit_errors::ErrorCode::NotFound);
}

#[tokio::test]
async fn store_list_with_limit() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    for i in 0..5 {
        let source = FileSource::from_bytes(Bytes::from(format!("item {i}")));
        store
            .upload(&source, &format!("items/{i}.txt"), None, None)
            .await
            .unwrap();
    }

    let limited = store.list("items", Some(3)).await.unwrap();
    assert!(limited.len() <= 3, "expected <= 3, got {}", limited.len());
}

#[tokio::test]
async fn store_list_empty_prefix_returns_root_files() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"root"));
    store.upload(&source, "root.txt", None, None).await.unwrap();

    // List with empty prefix — lists the root dir
    let items = store.list("", None).await.unwrap();
    assert!(
        !items.is_empty(),
        "expected at least 1, got {}",
        items.len()
    );
    assert!(items.iter().any(|item| item.key == "root.txt"));
    assert!(items.iter().all(|item| !item.key.starts_with('/')));
}

#[tokio::test]
async fn store_list_trailing_prefix_returns_normalized_keys() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"nested"));
    store
        .upload(&source, "items/file.txt", None, None)
        .await
        .unwrap();

    let items = store.list("items/", None).await.unwrap();
    assert!(items.iter().any(|item| item.key == "items/file.txt"));
    assert!(items.iter().all(|item| !item.key.contains("//")));
}

#[tokio::test]
async fn store_leading_slash_keys_are_resolved_under_root() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"rooted"));
    let stored = store
        .upload(&source, "/rooted.txt", None, None)
        .await
        .unwrap();

    assert_eq!(stored.key, "rooted.txt");
    assert!(dir.path().join("rooted.txt").exists());
    assert!(store.exists("rooted.txt").await.unwrap());
    assert_eq!(store.head("/rooted.txt").await.unwrap().key, "rooted.txt");

    let items = store.list("/", None).await.unwrap();
    assert!(items.iter().any(|item| item.key == "rooted.txt"));
    assert!(items.iter().all(|item| !item.key.starts_with('/')));
}

#[tokio::test]
async fn store_rejects_path_traversal_keys() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let source = FileSource::from_bytes(Bytes::from_static(b"escape"));

    assert!(
        store
            .upload(&source, "../escape.txt", None, None)
            .await
            .is_err()
    );
    assert!(store.download("../escape.txt").await.is_err());
    assert!(store.delete("../escape.txt").await.is_err());
    assert!(store.exists("../escape.txt").await.is_err());
    assert!(store.head("../escape.txt").await.is_err());
    assert!(store.list("../", None).await.is_err());
    assert!(
        store
            .presigned_url("../escape.txt", std::time::Duration::from_secs(60))
            .await
            .is_err()
    );

    store.upload(&source, "safe.txt", None, None).await.unwrap();
    assert!(store.copy("../escape.txt", "copy.txt").await.is_err());
    assert!(store.copy("safe.txt", "../copy.txt").await.is_err());
    assert!(store.rename("../escape.txt", "renamed.txt").await.is_err());
    assert!(store.rename("safe.txt", "../renamed.txt").await.is_err());
}

#[tokio::test]
async fn store_list_nonexistent_prefix_empty() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let items = store.list("nonexistent", None).await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn store_presigned_url_format() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    let url = store
        .presigned_url("test.txt", std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert!(url.starts_with("file://"));
    assert!(url.contains("test.txt"));
}

#[tokio::test]
async fn store_presigned_url_creates_missing_nested_parent() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let url = store
        .presigned_url("nested/new/test.txt", std::time::Duration::from_secs(60))
        .await
        .unwrap();

    assert!(url.starts_with("file://"));
    assert!(url.contains("nested/new/test.txt"));
    assert!(dir.path().join("nested/new").is_dir());
}

// ── Copy operations ────────────────────────────────────────────────────────

#[tokio::test]
async fn store_copy_preserves_content() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let data = Bytes::from_static(b"copy preserve content");
    let source = FileSource::from_bytes(data.clone());
    store
        .upload(&source, "orig.txt", Some("text/plain"), None)
        .await
        .unwrap();

    let copied = store.copy("orig.txt", "copy.txt").await.unwrap();
    assert_eq!(copied.key, "copy.txt");
    assert_eq!(copied.size, data.len() as u64);

    // Verify both exist with correct content
    let orig = store
        .download("orig.txt")
        .await
        .unwrap()
        .read_all()
        .await
        .unwrap();
    let copy = store
        .download("copy.txt")
        .await
        .unwrap()
        .read_all()
        .await
        .unwrap();
    assert_eq!(orig, copy);
}

#[tokio::test]
async fn store_copy_to_nested_dir() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"nested copy"));
    store.upload(&source, "flat.txt", None, None).await.unwrap();

    store
        .copy("flat.txt", "deep/nested/copy.txt")
        .await
        .unwrap();
    assert!(store.exists("deep/nested/copy.txt").await.unwrap());
}

#[tokio::test]
async fn store_rename_moves_file() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"rename content"));
    store
        .upload(&source, "before.txt", None, None)
        .await
        .unwrap();

    let renamed = store.rename("before.txt", "after.txt").await.unwrap();
    assert_eq!(renamed.key, "after.txt");
    assert!(!store.exists("before.txt").await.unwrap());
    assert!(store.exists("after.txt").await.unwrap());

    let content = store
        .download("after.txt")
        .await
        .unwrap()
        .read_all()
        .await
        .unwrap();
    assert_eq!(content.as_ref(), b"rename content");
}

#[tokio::test]
async fn store_rename_to_nested() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let source = FileSource::from_bytes(Bytes::from_static(b"nested rename"));
    store.upload(&source, "old.txt", None, None).await.unwrap();
    store.rename("old.txt", "sub/dir/new.txt").await.unwrap();
    assert!(store.exists("sub/dir/new.txt").await.unwrap());
}

// ── Large file handling ────────────────────────────────────────────────────

#[tokio::test]
async fn store_large_file_upload_download() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let size = 2 * 1024 * 1024; // 2MB
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let source = FileSource::from_bytes(Bytes::from(data.clone()));

    let stored = store
        .upload(&source, "large.bin", None, None)
        .await
        .unwrap();
    assert_eq!(stored.size, size as u64);

    let downloaded = store.download("large.bin").await.unwrap();
    let content = downloaded.read_all().await.unwrap();
    assert_eq!(content.len(), size);
    assert_eq!(content.as_ref(), data.as_slice());
}

#[tokio::test]
async fn source_stream_reads_all_data() {
    let data = vec![0xFFu8; 256 * 1024]; // 256KB
    let source = FileSource::from_bytes(Bytes::from(data.clone()));

    use futures::StreamExt;
    let stream = source.stream().await.unwrap();
    let chunks: Vec<_> = stream.collect().await;
    let total: Vec<u8> = chunks
        .into_iter()
        .filter_map(|r| r.ok())
        .flat_map(|b| b.to_vec())
        .collect();
    assert_eq!(total.len(), data.len());
}

// ── Concurrent file operations ─────────────────────────────────────────────

#[tokio::test]
async fn concurrent_store_uploads() {
    let dir = TempDir::new().unwrap();
    let store = std::sync::Arc::new(make_store(dir.path()));

    let mut handles = Vec::new();
    for i in 0..20 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            let data = format!("data-{i}");
            let source = FileSource::from_bytes(Bytes::from(data.clone()));
            store
                .upload(&source, &format!("concurrent/{i}.txt"), None, None)
                .await
                .unwrap();
            let downloaded = store
                .download(&format!("concurrent/{i}.txt"))
                .await
                .unwrap();
            let content = downloaded.read_all().await.unwrap();
            assert_eq!(content.as_ref(), data.as_bytes());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn concurrent_temp_file_creation() {
    let mut handles = Vec::new();
    for i in 0..20 {
        handles.push(tokio::spawn(async move {
            let tmp = TempFile::new().unwrap();
            let data = format!("temp-{i}");
            std::fs::write(tmp.path(), data.as_bytes()).unwrap();
            let content = std::fs::read(tmp.path()).unwrap();
            assert_eq!(content, data.as_bytes());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

// ── Config / backend validation ────────────────────────────────────────────

#[test]
fn local_store_config_auto_create_creates_dir() {
    let dir = TempDir::new().unwrap();
    let new_dir = dir.path().join("new_store");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: new_dir.clone(),
        auto_create: true,
    });
    assert!(store.is_ok());
    assert!(new_dir.exists());
}

#[test]
fn local_store_config_no_auto_create_existing_dir_ok() {
    let dir = TempDir::new().unwrap();
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: false,
    });
    assert!(store.is_ok());
}

#[test]
fn local_store_new_rejects_file_root() {
    let dir = TempDir::new().unwrap();
    let root_file = dir.path().join("root-file");
    std::fs::write(&root_file, b"not a directory").unwrap();

    let error = match store::LocalStore::new(store::LocalStoreConfig {
        root_dir: root_file,
        auto_create: false,
    }) {
        Ok(_) => panic!("file root should be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert!(error.to_string().contains("must be a directory"));
}

#[cfg(unix)]
#[test]
fn local_store_new_rejects_symlink_root() {
    let dir = TempDir::new().unwrap();
    let real_root = dir.path().join("real-root");
    let linked_root = dir.path().join("linked-root");
    std::fs::create_dir(&real_root).unwrap();
    std::os::unix::fs::symlink(&real_root, &linked_root).unwrap();

    let error = match store::LocalStore::new(store::LocalStoreConfig {
        root_dir: linked_root,
        auto_create: false,
    }) {
        Ok(_) => panic!("symlink root should be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert!(error.to_string().contains("must not be a symlink"));
}

#[cfg(unix)]
#[tokio::test]
async fn store_upload_rejects_symlink_parent_escape_without_writing_outside_root() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let store = make_store(dir.path());
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();

    let error = store
        .upload(
            &FileSource::from_bytes(Bytes::from_static(b"escape")),
            "link/escape.txt",
            None,
            None,
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert!(!outside.path().join("escape.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn store_presigned_url_rejects_existing_symlink_target() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());
    std::fs::write(dir.path().join("target.txt"), b"target").unwrap();
    std::os::unix::fs::symlink(dir.path().join("target.txt"), dir.path().join("link.txt")).unwrap();

    let error = store
        .presigned_url("link.txt", std::time::Duration::from_secs(60))
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn store_upload_with_progress() {
    let dir = TempDir::new().unwrap();
    let store = make_store(dir.path());

    let progress_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let progress_cb = progress_called.clone();

    let cb: ProgressCallback = std::sync::Arc::new(move |_progress| {
        progress_cb.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let source = FileSource::from_bytes(Bytes::from_static(b"progress"));
    let result = store
        .upload_with_progress(&source, "prog.txt", None, cb)
        .await;
    assert!(result.is_ok());
    // Verify file was written
    assert!(store.exists("prog.txt").await.unwrap());
}

// ── copy_file helper ───────────────────────────────────────────────────────

#[tokio::test]
async fn copy_file_bytes_to_temp() {
    let source = FileSource::from_bytes(Bytes::from_static(b"to temp"));
    let sink = FileSink::Temp;
    let result = copy_file(&source, &sink, None).await.unwrap();
    let content = result.read_all().await.unwrap();
    assert_eq!(content.as_ref(), b"to temp");
}

#[tokio::test]
async fn copy_file_path_to_memory() {
    let tmp = TempFile::new().unwrap();
    std::fs::write(tmp.path(), b"path to mem").unwrap();
    let source = FileSource::from_path(tmp.path());
    let sink = FileSink::Memory;
    let result = copy_file(&source, &sink, None).await.unwrap();
    let content = result.read_all().await.unwrap();
    assert_eq!(content.as_ref(), b"path to mem");
}

#[tokio::test]
async fn copy_file_temp_to_path() {
    let src_tmp = TempFile::new().unwrap();
    std::fs::write(src_tmp.path(), b"temp to path").unwrap();
    let source = FileSource::Temp(src_tmp);

    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("output.bin");
    let sink = FileSink::Path(dest.clone());

    let result = copy_file(&source, &sink, None).await.unwrap();
    let content = result.read_all().await.unwrap();
    assert_eq!(content.as_ref(), b"temp to path");
}

#[tokio::test]
async fn transfer_between_local_stores_copies_content_and_detected_content_type() {
    let from_dir = TempDir::new().unwrap();
    let to_dir = TempDir::new().unwrap();
    let from_store = make_store(from_dir.path());
    let to_store = make_store(to_dir.path());
    let source = FileSource::from_bytes(Bytes::from_static(b"transfer content"));
    from_store
        .upload(&source, "input.txt", Some("text/plain"), None)
        .await
        .unwrap();

    let stored = transfer(&from_store, "input.txt", &to_store, "nested/output.txt")
        .await
        .unwrap();

    assert_eq!(stored.key, "nested/output.txt");
    assert_eq!(stored.size, b"transfer content".len() as u64);
    assert_eq!(stored.content_type, "text/plain");
    assert!(from_store.exists("input.txt").await.unwrap());
    let copied = to_store
        .download("nested/output.txt")
        .await
        .unwrap()
        .read_all()
        .await
        .unwrap();
    assert_eq!(copied.as_ref(), b"transfer content");
}

#[tokio::test]
async fn transfer_missing_source_does_not_create_destination() {
    let from_dir = TempDir::new().unwrap();
    let to_dir = TempDir::new().unwrap();
    let from_store = make_store(from_dir.path());
    let to_store = make_store(to_dir.path());

    let error = transfer(&from_store, "missing.txt", &to_store, "created.txt")
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::NotFound);
    assert!(!to_store.exists("created.txt").await.unwrap());
}

// ── UploadProgress struct ──────────────────────────────────────────────────

#[test]
fn upload_progress_fields() {
    let progress = UploadProgress {
        bytes_sent: 500,
        total_bytes: Some(1000),
        percent: Some(50.0),
    };
    assert_eq!(progress.bytes_sent, 500);
    assert_eq!(progress.total_bytes, Some(1000));
    assert_eq!(progress.percent, Some(50.0));
}

#[test]
fn upload_progress_unknown_total() {
    let progress = UploadProgress {
        bytes_sent: 100,
        total_bytes: None,
        percent: None,
    };
    assert_eq!(progress.total_bytes, None);
    assert_eq!(progress.percent, None);
}

// ── StoredFile struct ──────────────────────────────────────────────────────

#[test]
fn stored_file_fields() {
    let stored = StoredFile {
        key: "test/file.txt".to_string(),
        size: 42,
        content_type: "text/plain".to_string(),
        stored_at: chrono::Utc::now(),
        metadata: HashMap::new(),
    };
    assert_eq!(stored.key, "test/file.txt");
    assert_eq!(stored.size, 42);
    assert_eq!(stored.content_type, "text/plain");
}

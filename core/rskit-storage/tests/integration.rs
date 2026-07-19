//! Integration tests for rskit-storage: FileSource, FileSink, TempFile, meta, LocalStore.

use rskit_storage::*;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Create a small PNG in memory using raw bytes (1×1 red pixel).
fn tiny_png_bytes() -> Vec<u8> {
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(
        encoder,
        &[255, 0, 0], // RGB red
        1,
        1,
        image::ExtendedColorType::Rgb8,
    )
    .expect("encode tiny PNG");
    buf
}

/// Create a small JPEG in memory (1×1 blue pixel).
fn tiny_jpeg_bytes() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .expect("encode tiny JPEG");
    buf.into_inner()
}

/// Write bytes to a temp file and return the path.
fn write_temp_fixture(data: &[u8], ext: &str) -> TempFile {
    let tmp = TempFile::with_extension(ext).expect("create temp");
    std::fs::write(tmp.path(), data).expect("write fixture");
    tmp
}

// ── FileSource ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn file_source_from_bytes_read_all() {
    let data = b"hello world";
    let source = FileSource::from_bytes(bytes::Bytes::from_static(data));
    let result = source.read_all().await.expect("read_all");
    assert_eq!(result.as_ref(), data);
}

#[tokio::test]
async fn file_source_from_path_read_all() {
    let tmp = write_temp_fixture(b"file content", "txt");
    let source = FileSource::from_path(tmp.path());
    let result = source.read_all().await.expect("read_all");
    assert_eq!(result.as_ref(), b"file content");
}

#[tokio::test]
async fn file_source_size_bytes() {
    let data = bytes::Bytes::from_static(b"twelve chars");
    let source = FileSource::from_bytes(data);
    let size = source.size().await.expect("size");
    assert_eq!(size, Some(12));
}

#[tokio::test]
async fn file_source_size_path() {
    let tmp = write_temp_fixture(b"hello", "txt");
    let source = FileSource::from_path(tmp.path());
    let size = source.size().await.expect("size");
    assert_eq!(size, Some(5));
}

#[tokio::test]
async fn file_source_extension() {
    let source = FileSource::from_path("/tmp/test.mp4");
    assert_eq!(source.extension(), Some("mp4"));

    let source2 = FileSource::from_path("/tmp/noext");
    assert_eq!(source2.extension(), None);
}

// ── FileSink ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn file_sink_path_write_and_finalize() {
    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("output.txt");
    let sink = FileSink::Path(out_path.clone());
    let mut writer = sink.writer().await.expect("writer");
    writer.write_all(b"hello sink").await.expect("write");
    let result = writer.finalize().await.expect("finalize");

    // Should return a FileSource::Path pointing to the output
    match &result {
        FileSource::Path(p) => assert_eq!(p, &out_path),
        _ => panic!("expected FileSource::Path"),
    }
    let content = std::fs::read_to_string(&out_path).expect("read back");
    assert_eq!(content, "hello sink");
}

#[tokio::test]
async fn file_sink_memory_write_and_finalize() {
    let sink = FileSink::Memory;
    let mut writer = sink.writer().await.expect("writer");
    writer.write_all(b"in memory").await.expect("write");
    let result = writer.finalize().await.expect("finalize");

    match result {
        FileSource::Bytes(b) => assert_eq!(b.as_ref(), b"in memory"),
        _ => panic!("expected FileSource::Bytes"),
    }
}

#[tokio::test]
async fn file_sink_temp_write_and_finalize() {
    let sink = FileSink::Temp;
    let mut writer = sink.writer().await.expect("writer");
    writer.write_all(b"temp data").await.expect("write");
    let result = writer.finalize().await.expect("finalize");

    // Should be FileSource::Temp or FileSource::Path
    let data = result.read_all().await.expect("read back");
    assert_eq!(data.as_ref(), b"temp data");
}

// ── TempFile ────────────────────────────────────────────────────────────────

#[test]
fn temp_file_new() {
    let tmp = TempFile::new().expect("new temp");
    assert!(tmp.path().exists());
}

#[test]
fn temp_file_with_extension() {
    let tmp = TempFile::with_extension("png").expect("temp png");
    assert!(tmp.path().to_string_lossy().ends_with(".png"));
}

#[test]
fn temp_file_persist() {
    let tmp = TempFile::new().expect("temp");
    std::fs::write(tmp.path(), b"persist me").expect("write");
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("persisted.dat");
    let persisted = tmp.persist(&target).expect("persist");
    assert_eq!(persisted, target);
    assert!(target.exists());
    let content = std::fs::read_to_string(&target).expect("read");
    assert_eq!(content, "persist me");
}

#[test]
fn temp_file_into_source() {
    let tmp = TempFile::new().expect("temp");
    std::fs::write(tmp.path(), b"source").expect("write");
    let source = tmp.into_source();
    match source {
        FileSource::Temp(_) => {}
        _ => panic!("expected FileSource::Temp"),
    }
}

// ── TempDir ─────────────────────────────────────────────────────────────────

#[test]
fn temp_dir_new() {
    let dir = TempDir::new().expect("new temp dir");
    assert!(dir.path().is_dir());
}

#[test]
fn temp_dir_create_file() {
    let dir = TempDir::new().expect("dir");
    let f = dir.create_file("test.txt").expect("create file");
    assert!(f.path().exists());
    assert!(f.path().starts_with(dir.path()));
}

// ── MIME detection ──────────────────────────────────────────────────────────

#[tokio::test]
async fn detect_mime_png() {
    let png_data = tiny_png_bytes();
    let tmp = write_temp_fixture(&png_data, "png");
    let source = FileSource::from_path(tmp.path());
    let mime = detect_mime(&source).await.expect("detect mime");
    assert!(mime.contains("png"), "expected PNG mime, got: {mime}");
}

#[tokio::test]
async fn detect_mime_jpeg() {
    let jpeg_data = tiny_jpeg_bytes();
    let tmp = write_temp_fixture(&jpeg_data, "jpg");
    let source = FileSource::from_path(tmp.path());
    let mime = detect_mime(&source).await.expect("detect mime");
    assert!(
        mime.contains("jpeg") || mime.contains("jpg"),
        "expected JPEG mime, got: {mime}"
    );
}

#[tokio::test]
async fn detect_kind_image() {
    let png_data = tiny_png_bytes();
    let tmp = write_temp_fixture(&png_data, "png");
    let source = FileSource::from_path(tmp.path());
    let kind = detect_kind(&source).await.expect("detect kind");
    assert_eq!(kind, FileKind::Image);
}

#[tokio::test]
async fn detect_kind_text() {
    let tmp = write_temp_fixture(b"hello world plain text", "txt");
    let source = FileSource::from_path(tmp.path());
    let kind = detect_kind(&source).await.expect("detect kind");
    // Text detection depends on extension primarily
    assert!(
        kind == FileKind::Text || kind == FileKind::Unknown || kind == FileKind::Binary,
        "got: {kind:?}"
    );
}

#[tokio::test]
async fn file_meta_on_png() {
    let png_data = tiny_png_bytes();
    let tmp = write_temp_fixture(&png_data, "png");
    let source = FileSource::from_path(tmp.path());
    let meta = file_meta(&source).await.expect("file_meta");
    assert!(meta.mime_type.contains("png"));
    assert_eq!(meta.extension, Some("png".into()));
    assert!(meta.size.unwrap() > 0);
}

// ── LocalStore ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn local_store_upload_download() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    let data = bytes::Bytes::from_static(b"store test content");
    let source = FileSource::from_bytes(data.clone());
    let stored = store
        .upload(&source, "test/file.txt", UploadOptions::new().with_content_type("text/plain"))
        .await
        .expect("upload");

    assert_eq!(stored.key, "test/file.txt");
    assert_eq!(stored.size, data.len() as u64);

    // Download
    let downloaded = store.download("test/file.txt").await.expect("download");
    let content = downloaded.read_all().await.expect("read");
    assert_eq!(content.as_ref(), b"store test content");
}

#[tokio::test]
async fn local_store_exists() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    assert!(!store.exists("nope.txt").await.expect("exists check"));

    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"hi"));
    store
        .upload(&source, "yes.txt", UploadOptions::new())
        .await
        .expect("upload");

    assert!(store.exists("yes.txt").await.expect("exists check"));
}

#[tokio::test]
async fn local_store_delete() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"delete me"));
    store
        .upload(&source, "del.txt", UploadOptions::new())
        .await
        .expect("upload");
    assert!(store.exists("del.txt").await.unwrap());

    store.delete("del.txt").await.expect("delete");
    assert!(!store.exists("del.txt").await.unwrap());
}

#[tokio::test]
async fn local_store_list() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    for name in &["a.txt", "b.txt", "c.txt"] {
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"x"));
        store
            .upload(&source, name, UploadOptions::new())
            .await
            .expect("upload");
    }

    let all = store.list("", None).await.expect("list all");
    assert!(
        all.len() >= 3,
        "expected at least 3, got {} items",
        all.len()
    );
}

#[tokio::test]
async fn local_store_copy() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"copy me"));
    store
        .upload(&source, "original.txt", UploadOptions::new())
        .await
        .expect("upload");

    store
        .copy("original.txt", "copied.txt")
        .await
        .expect("copy");
    assert!(store.exists("original.txt").await.unwrap());
    assert!(store.exists("copied.txt").await.unwrap());

    let content = store
        .download("copied.txt")
        .await
        .unwrap()
        .read_all()
        .await
        .unwrap();
    assert_eq!(content.as_ref(), b"copy me");
}

#[tokio::test]
async fn local_store_rename() {
    let dir = TempDir::new().expect("store dir");
    let store = store::LocalStore::new(store::LocalStoreConfig {
        root_dir: dir.path().to_path_buf(),
        auto_create: true,
    })
    .expect("create store");

    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"rename me"));
    store
        .upload(&source, "old.txt", UploadOptions::new())
        .await
        .expect("upload");

    store.rename("old.txt", "new.txt").await.expect("rename");
    assert!(!store.exists("old.txt").await.unwrap());
    assert!(store.exists("new.txt").await.unwrap());
}

// ── copy_file ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn copy_file_path_to_path() {
    let src_tmp = write_temp_fixture(b"copy test", "txt");
    let source = FileSource::from_path(src_tmp.path());

    let dir = TempDir::new().expect("dest dir");
    let dest_path = dir.path().join("copied.txt");
    let sink = FileSink::Path(dest_path.clone());

    let result = copy_file(&source, &sink, None).await.expect("copy_file");
    let content = result.read_all().await.expect("read");
    assert_eq!(content.as_ref(), b"copy test");
}

#[tokio::test]
async fn copy_file_bytes_to_memory() {
    let source = FileSource::from_bytes(bytes::Bytes::from_static(b"memory copy"));
    let sink = FileSink::Memory;

    let result = copy_file(&source, &sink, None).await.expect("copy_file");
    let content = result.read_all().await.expect("read");
    assert_eq!(content.as_ref(), b"memory copy");
}

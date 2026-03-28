use std::path::PathBuf;

use rskit_file::{detect_kind, detect_mime, file_meta, FileSource};

fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
        .join(rel)
}

fn fixture_source(rel: &str) -> FileSource {
    FileSource::from_path(fixture_path(rel))
}

// ── MIME Detection ──────────────────────────────────────────────────

#[tokio::test]
async fn mime_detection_ai_generated_jpg() {
    let mime = detect_mime(&fixture_source("image/ai-generated.jpg"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_ai_generated_jpg", mime);
}

#[tokio::test]
async fn mime_detection_real_photo_jpg() {
    let mime = detect_mime(&fixture_source("image/real-photo.jpg"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_real_photo_jpg", mime);
}

#[tokio::test]
async fn mime_detection_sample_png() {
    let mime = detect_mime(&fixture_source("image/sample.png"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_sample_png", mime);
}

#[tokio::test]
async fn mime_detection_ai_generated_wav() {
    let mime = detect_mime(&fixture_source("audio/ai-generated.wav"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_ai_generated_wav", mime);
}

#[tokio::test]
async fn mime_detection_real_voice_wav() {
    let mime = detect_mime(&fixture_source("audio/real-voice.wav"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_real_voice_wav", mime);
}

#[tokio::test]
async fn mime_detection_ai_generated_mp4() {
    let mime = detect_mime(&fixture_source("video/ai-generated.mp4"))
        .await
        .unwrap();
    insta::assert_snapshot!("mime_detection_ai_generated_mp4", mime);
}

// ── FileKind Detection ──────────────────────────────────────────────

#[tokio::test]
async fn file_kind_ai_generated_jpg() {
    let kind = detect_kind(&fixture_source("image/ai-generated.jpg"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_ai_generated_jpg", kind);
}

#[tokio::test]
async fn file_kind_real_photo_jpg() {
    let kind = detect_kind(&fixture_source("image/real-photo.jpg"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_real_photo_jpg", kind);
}

#[tokio::test]
async fn file_kind_sample_png() {
    let kind = detect_kind(&fixture_source("image/sample.png"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_sample_png", kind);
}

#[tokio::test]
async fn file_kind_ai_generated_wav() {
    let kind = detect_kind(&fixture_source("audio/ai-generated.wav"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_ai_generated_wav", kind);
}

#[tokio::test]
async fn file_kind_real_voice_wav() {
    let kind = detect_kind(&fixture_source("audio/real-voice.wav"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_real_voice_wav", kind);
}

#[tokio::test]
async fn file_kind_ai_generated_mp4() {
    let kind = detect_kind(&fixture_source("video/ai-generated.mp4"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_kind_ai_generated_mp4", kind);
}

// ── FileMeta ────────────────────────────────────────────────────────

#[tokio::test]
async fn file_meta_ai_generated_jpg() {
    let meta = file_meta(&fixture_source("image/ai-generated.jpg"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_ai_generated_jpg", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[tokio::test]
async fn file_meta_real_photo_jpg() {
    let meta = file_meta(&fixture_source("image/real-photo.jpg"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_real_photo_jpg", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[tokio::test]
async fn file_meta_sample_png() {
    let meta = file_meta(&fixture_source("image/sample.png"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_sample_png", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[tokio::test]
async fn file_meta_ai_generated_wav() {
    let meta = file_meta(&fixture_source("audio/ai-generated.wav"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_ai_generated_wav", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[tokio::test]
async fn file_meta_real_voice_wav() {
    let meta = file_meta(&fixture_source("audio/real-voice.wav"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_real_voice_wav", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[tokio::test]
async fn file_meta_ai_generated_mp4() {
    let meta = file_meta(&fixture_source("video/ai-generated.mp4"))
        .await
        .unwrap();
    insta::assert_json_snapshot!("file_meta_ai_generated_mp4", meta, {
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

// ── FileSource ──────────────────────────────────────────────────────

#[tokio::test]
async fn file_source_jpg_extension_and_size() {
    let source = fixture_source("image/ai-generated.jpg");
    let ext = source.extension().unwrap_or("none").to_string();
    let size = source.size().await.unwrap();
    insta::assert_snapshot!("file_source_extension_jpg", ext);
    insta::assert_debug_snapshot!("file_source_size_ai_generated_jpg", size);
}

#[tokio::test]
async fn file_source_png_extension_and_size() {
    let source = fixture_source("image/sample.png");
    let ext = source.extension().unwrap_or("none").to_string();
    let size = source.size().await.unwrap();
    insta::assert_snapshot!("file_source_extension_png", ext);
    insta::assert_debug_snapshot!("file_source_size_sample_png", size);
}

#[tokio::test]
async fn file_source_wav_extension_and_size() {
    let source = fixture_source("audio/ai-generated.wav");
    let ext = source.extension().unwrap_or("none").to_string();
    let size = source.size().await.unwrap();
    insta::assert_snapshot!("file_source_extension_wav", ext);
    insta::assert_debug_snapshot!("file_source_size_ai_generated_wav", size);
}

#[tokio::test]
async fn file_source_mp4_extension_and_size() {
    let source = fixture_source("video/ai-generated.mp4");
    let ext = source.extension().unwrap_or("none").to_string();
    let size = source.size().await.unwrap();
    insta::assert_snapshot!("file_source_extension_mp4", ext);
    insta::assert_debug_snapshot!("file_source_size_ai_generated_mp4", size);
}

#[tokio::test]
async fn file_source_read_all_returns_bytes() {
    let source = fixture_source("image/ai-generated.jpg");
    let bytes = source.read_all().await.unwrap();
    assert!(!bytes.is_empty());
    insta::assert_snapshot!("file_source_read_all_len_ai_generated_jpg", bytes.len().to_string());
}

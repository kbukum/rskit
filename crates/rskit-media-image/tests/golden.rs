//! Golden/snapshot tests for rskit-media-image using real fixture files.

use std::path::PathBuf;

use rskit_file::FileSource;
use rskit_media::{
    executor::MediaExecutor,
    format::Format,
    ops::{CropRegion, MediaOp, ResizeMode, ResizeOp, Rotation},
    output::OutputConfig,
    spatial::Resolution,
};
use rskit_media_image::ImageProcessor;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

fn read_dimensions(source: &FileSource) -> (u32, u32) {
    match source {
        FileSource::Bytes(b) => {
            let img = image::load_from_memory(b).expect("load from bytes");
            (img.width(), img.height())
        }
        FileSource::Path(p) => {
            let img = image::open(p).expect("open image");
            (img.width(), img.height())
        }
        FileSource::Temp(t) => {
            let img = image::open(t.path()).expect("open temp image");
            (img.width(), img.height())
        }
        _ => panic!("unsupported source type"),
    }
}

fn source_bytes(source: &FileSource) -> Vec<u8> {
    match source {
        FileSource::Bytes(b) => b.to_vec(),
        FileSource::Path(p) => std::fs::read(p).expect("read file"),
        FileSource::Temp(t) => std::fs::read(t.path()).expect("read temp file"),
        _ => panic!("unsupported source type"),
    }
}

// ── Test 1: Real image processing ────────────────────────────────────────────

#[tokio::test]
async fn golden_resize_real_jpeg() {
    let source = FileSource::from_path(fixtures_dir().join("image/real-photo.jpg"));
    let processor = ImageProcessor::new();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 100),
        mode: ResizeMode::Exact,
    })];

    let result = processor.execute(&source, &ops, None).await.unwrap();
    let (w, h) = read_dimensions(&result);

    insta::assert_json_snapshot!(
        "resize_real_jpeg",
        serde_json::json!({
            "width": w,
            "height": h,
            "format": "jpeg",
        })
    );
}

// ── Test 2: Multi-format processing ──────────────────────────────────────────

#[tokio::test]
async fn golden_multi_format_pipeline() {
    let processor = ImageProcessor::new();
    let ops = vec![
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(150, 150),
            mode: ResizeMode::Exact,
        }),
        MediaOp::Crop(CropRegion::new(25, 25, 100, 100)),
    ];

    // Process JPEG
    let jpeg_source = FileSource::from_path(fixtures_dir().join("image/real-photo.jpg"));
    let jpeg_result = processor.execute(&jpeg_source, &ops, None).await.unwrap();
    let (jw, jh) = read_dimensions(&jpeg_result);

    // Process PNG
    let png_source = FileSource::from_path(fixtures_dir().join("image/sample.png"));
    let png_result = processor.execute(&png_source, &ops, None).await.unwrap();
    let (pw, ph) = read_dimensions(&png_result);

    insta::assert_json_snapshot!(
        "multi_format_pipeline",
        serde_json::json!({
            "jpeg": { "width": jw, "height": jh },
            "png":  { "width": pw, "height": ph },
        })
    );
}

// ── Test 3: Crop real image ──────────────────────────────────────────────────

#[tokio::test]
async fn golden_crop_center_real_photo() {
    let source = FileSource::from_path(fixtures_dir().join("image/real-photo.jpg"));
    let processor = ImageProcessor::new();

    // real-photo.jpg is 500x378 — center crop 100x100
    let crop = CropRegion::center(Resolution::new(500, 378), 100, 100);
    let ops = vec![MediaOp::Crop(crop)];

    let result = processor.execute(&source, &ops, None).await.unwrap();
    let (w, h) = read_dimensions(&result);

    insta::assert_json_snapshot!(
        "crop_center_real_photo",
        serde_json::json!({
            "width": w,
            "height": h,
        })
    );
}

// ── Test 4: Rotate real image ────────────────────────────────────────────────

#[tokio::test]
async fn golden_rotate_90() {
    // ai-generated.jpg is 270x270 (square), so rotation doesn't change dims.
    // Use real-photo.jpg (500x378) to see the swap.
    let source = FileSource::from_path(fixtures_dir().join("image/real-photo.jpg"));
    let processor = ImageProcessor::new();
    let ops = vec![MediaOp::Rotate(Rotation::Degrees90)];

    let result = processor.execute(&source, &ops, None).await.unwrap();
    let (w, h) = read_dimensions(&result);

    // 500x378 rotated 90° → 378x500
    insta::assert_json_snapshot!(
        "rotate_90_real_photo",
        serde_json::json!({
            "original_width": 500,
            "original_height": 378,
            "rotated_width": w,
            "rotated_height": h,
        })
    );
}

// ── Test 5: Format conversion ────────────────────────────────────────────────

#[tokio::test]
async fn golden_png_to_jpeg_conversion() {
    let source = FileSource::from_path(fixtures_dir().join("image/sample.png"));
    let processor = ImageProcessor::new();
    let ops = vec![MediaOp::Transcode(OutputConfig::new(Format::new("jpeg")))];

    let result = processor.execute(&source, &ops, None).await.unwrap();
    let bytes = source_bytes(&result);

    // Verify it's actually JPEG by checking magic bytes
    let is_jpeg = bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8;
    let (w, h) = read_dimensions(&result);

    insta::assert_json_snapshot!("png_to_jpeg_conversion", {
        ".size_bytes" => insta::rounded_redaction(0),
    }, serde_json::json!({
        "is_jpeg": is_jpeg,
        "width": w,
        "height": h,
        "size_bytes": bytes.len(),
        "size_reasonable": bytes.len() > 1000 && bytes.len() < 500_000,
    }));
}

// ── Test 6: Pipeline on real data ────────────────────────────────────────────

#[tokio::test]
async fn golden_pipeline_resize_crop_rotate() {
    let source = FileSource::from_path(fixtures_dir().join("image/real-photo.jpg"));
    let processor = ImageProcessor::new();
    let ops = vec![
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(200, 200),
            mode: ResizeMode::Exact,
        }),
        MediaOp::Crop(CropRegion::center(Resolution::new(200, 200), 100, 100)),
        MediaOp::Rotate(Rotation::Degrees90),
    ];

    let result = processor.execute(&source, &ops, None).await.unwrap();
    let (w, h) = read_dimensions(&result);

    // 200x200 → crop center 100x100 → rotate 90° → 100x100 (square stays square)
    insta::assert_json_snapshot!(
        "pipeline_resize_crop_rotate",
        serde_json::json!({
            "final_width": w,
            "final_height": h,
            "pipeline_steps": ["resize(200x200)", "crop(100x100)", "rotate(90)"],
        })
    );
}

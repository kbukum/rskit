//! Integration tests for rskit-media-image with real image fixtures.

use image::{ImageFormat, Rgb, RgbImage};
use rskit_media::{
    Registry,
    executor::MediaExecutor,
    filter::{Filter, FilterTarget, ParamValue, Params},
    format::Format,
    ops::{CropRegion, FlipDirection, MediaOp, ResizeMode, ResizeOp, Rotation},
    output::OutputConfig,
    spatial::Resolution,
};
use rskit_storage::{FileSink, FileSource, TempDir, TempFile};
use std::sync::Arc;

// ── Fixture generation ──────────────────────────────────────────────────────

/// Create a 100×100 gradient PNG fixture.
fn create_gradient_png(width: u32, height: u32) -> TempFile {
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width) as u8;
            let g = (y * 255 / height) as u8;
            let b = 128u8;
            img.put_pixel(x, y, Rgb([r, g, b]));
        }
    }
    let tmp = TempFile::with_extension("png").expect("create temp");
    img.save(tmp.path()).expect("save PNG");
    tmp
}

/// Create a JPEG fixture.
#[allow(dead_code)]
fn create_jpeg(width: u32, height: u32) -> TempFile {
    let img = RgbImage::from_pixel(width, height, Rgb([100, 150, 200]));
    let tmp = TempFile::with_extension("jpg").expect("create temp");
    img.save(tmp.path()).expect("save JPEG");
    tmp
}

fn image_executor() -> Arc<dyn MediaExecutor> {
    let mut registry = Registry::default();
    rskit_media_image::register(&mut registry, rskit_media_image::Config)
        .expect("register image backend");
    registry.executor("image").expect("image executor")
}

/// Read image dimensions from a FileSource.
fn read_dimensions(source: &FileSource) -> (u32, u32) {
    match source {
        FileSource::Path(p) => {
            let img = image::open(p).expect("open image");
            (img.width(), img.height())
        }
        FileSource::Bytes(b) => {
            let img = image::load_from_memory(b.as_ref()).expect("load from bytes");
            (img.width(), img.height())
        }
        FileSource::Temp(t) => {
            let img = image::open(t.path()).expect("open temp image");
            (img.width(), img.height())
        }
        _ => panic!("unsupported source type"),
    }
}

/// Read image format from bytes.
#[allow(dead_code)]
fn detect_image_format(source: &FileSource) -> ImageFormat {
    match source {
        FileSource::Path(p) => ImageFormat::from_path(p).expect("detect format"),
        FileSource::Bytes(b) => image::guess_format(b).expect("guess format"),
        FileSource::Temp(t) => ImageFormat::from_path(t.path()).expect("detect format from temp"),
        _ => panic!("unsupported source type"),
    }
}

// ── Resize tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn resize_exact() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(50, 30),
        mode: ResizeMode::Exact,
    })];

    let result = backend.execute(&source, &ops, None).await.expect("resize");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 30));
}

#[tokio::test]
async fn resize_fit() {
    let fixture = create_gradient_png(200, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 100),
        mode: ResizeMode::Fit,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fit");
    let (w, h) = read_dimensions(&result);
    // 200×100 fitting into 100×100 → should be 100×50 (preserving 2:1 ratio)
    assert!(w <= 100 && h <= 100, "got: {w}×{h}");
    assert_eq!(w, 100);
    assert_eq!(h, 50);
}

#[tokio::test]
async fn resize_fill() {
    let fixture = create_gradient_png(200, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 100),
        mode: ResizeMode::Fill,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fill");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (100, 100));
}

#[tokio::test]
async fn resize_fit_width() {
    let fixture = create_gradient_png(200, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(100, 0),
        mode: ResizeMode::FitWidth,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fit width");
    let (w, h) = read_dimensions(&result);
    assert_eq!(w, 100);
    assert_eq!(h, 50, "should maintain 2:1 ratio");
}

#[tokio::test]
async fn resize_fit_height() {
    let fixture = create_gradient_png(200, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(0, 50),
        mode: ResizeMode::FitHeight,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fit height");
    let (w, h) = read_dimensions(&result);
    assert_eq!(h, 50);
    assert_eq!(w, 100, "should maintain 2:1 ratio");
}

// ── Crop tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn crop_center() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Crop(CropRegion::new(10, 10, 50, 40))];

    let result = backend.execute(&source, &ops, None).await.expect("crop");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 40));
}

// ── Rotate tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn rotate_90() {
    let fixture = create_gradient_png(100, 50); // landscape
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Rotate(Rotation::Degrees90)];
    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("rotate 90");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 100), "90° rotation should swap dimensions");
}

#[tokio::test]
async fn rotate_180() {
    let fixture = create_gradient_png(100, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Rotate(Rotation::Degrees180)];
    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("rotate 180");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (100, 50), "180° should keep same dimensions");
}

#[tokio::test]
async fn rotate_270() {
    let fixture = create_gradient_png(100, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Rotate(Rotation::Degrees270)];
    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("rotate 270");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 100));
}

// ── Flip tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn flip_horizontal() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Flip(FlipDirection::Horizontal)];
    let result = backend.execute(&source, &ops, None).await.expect("flip h");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (100, 100), "flip should not change dimensions");
}

#[tokio::test]
async fn flip_vertical() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Flip(FlipDirection::Vertical)];
    let result = backend.execute(&source, &ops, None).await.expect("flip v");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (100, 100));
}

// ── Filter tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn filter_grayscale() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Filter(Filter {
        name: "grayscale".into(),
        target: FilterTarget::Video,
        params: Params::new(),
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("grayscale");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 50));
}

#[tokio::test]
async fn filter_blur() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Filter(Filter {
        name: "blur".into(),
        target: FilterTarget::Video,
        params: Params::new().set("radius", ParamValue::Float(2.0)),
    })];

    let result = backend.execute(&source, &ops, None).await.expect("blur");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 50));
}

#[tokio::test]
async fn filter_brightness() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Filter(Filter {
        name: "brightness".into(),
        target: FilterTarget::Video,
        params: Params::new().set("value", ParamValue::Int(30)),
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("brightness");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 50));
}

#[tokio::test]
async fn filter_contrast() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Filter(Filter {
        name: "contrast".into(),
        target: FilterTarget::Video,
        params: Params::new().set("value", ParamValue::Float(1.5)),
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("contrast");
    let (w, h) = read_dimensions(&result);
    assert_eq!((w, h), (50, 50));
}

// ── Format transcoding ─────────────────────────────────────────────────────

#[tokio::test]
async fn transcode_png_to_jpeg() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("output.jpg");
    let sink = FileSink::Path(out_path.clone());

    let config = OutputConfig {
        format: Format::new("jpeg"),
        video: None,
        audio: None,
        streaming: None,
        strip_metadata: false,
        extra: Default::default(),
    };

    let ops = vec![MediaOp::Transcode(config)];
    let result = backend
        .execute(&source, &ops, Some(&sink))
        .await
        .expect("transcode");

    match &result {
        FileSource::Path(p) => {
            let fmt = ImageFormat::from_path(p).expect("detect output format");
            assert_eq!(fmt, ImageFormat::Jpeg, "output should be JPEG");
        }
        _ => panic!("expected Path result"),
    }
}

#[tokio::test]
async fn transcode_png_to_webp() {
    let fixture = create_gradient_png(100, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("output.webp");
    let sink = FileSink::Path(out_path.clone());

    let config = OutputConfig {
        format: Format::new("webp"),
        video: None,
        audio: None,
        streaming: None,
        strip_metadata: false,
        extra: Default::default(),
    };

    let ops = vec![MediaOp::Transcode(config)];
    let result = backend
        .execute(&source, &ops, Some(&sink))
        .await
        .expect("transcode webp");

    // Verify output file exists and is valid
    match &result {
        FileSource::Path(p) => assert!(p.exists()),
        _ => panic!("expected Path result"),
    }
}

// ── Multi-op pipeline ───────────────────────────────────────────────────────

#[tokio::test]
async fn multi_op_resize_then_crop_then_rotate() {
    let fixture = create_gradient_png(200, 200);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(100, 100),
            mode: ResizeMode::Exact,
        }),
        MediaOp::Crop(CropRegion::new(10, 10, 50, 50)),
        MediaOp::Rotate(Rotation::Degrees90),
    ];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("multi-op");
    let (w, h) = read_dimensions(&result);
    // 200×200 → 100×100 → crop 50×50 → rotate 90° → 50×50
    assert_eq!((w, h), (50, 50));
}

// ── Output sinks ────────────────────────────────────────────────────────────

#[tokio::test]
async fn output_to_path() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let dir = TempDir::new().expect("temp dir");
    let out_path = dir.path().join("result.png");
    let sink = FileSink::Path(out_path.clone());

    let ops = vec![MediaOp::Flip(FlipDirection::Horizontal)];
    let result = backend
        .execute(&source, &ops, Some(&sink))
        .await
        .expect("output path");

    match result {
        FileSource::Path(p) => {
            assert_eq!(p, out_path);
            assert!(p.exists());
        }
        _ => panic!("expected Path result"),
    }
}

#[tokio::test]
async fn output_to_memory() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Flip(FlipDirection::Horizontal)];
    let result = backend
        .execute(&source, &ops, Some(&FileSink::Memory))
        .await
        .expect("output memory");

    match &result {
        FileSource::Bytes(b) => {
            assert!(!b.is_empty());
            // Should be a valid PNG
            let img = image::load_from_memory(b).expect("load from bytes");
            assert_eq!((img.width(), img.height()), (50, 50));
        }
        _ => panic!("expected Bytes result"),
    }
}

#[tokio::test]
async fn output_to_temp() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Flip(FlipDirection::Horizontal)];
    let result = backend
        .execute(&source, &ops, Some(&FileSink::Temp))
        .await
        .expect("output temp");

    match &result {
        FileSource::Temp(t) => assert!(t.path().exists()),
        _ => panic!(
            "expected Temp result, got: {:?}",
            std::mem::discriminant(&result)
        ),
    }
}

// ── Supports / error handling ───────────────────────────────────────────────

#[test]
fn supports_image_ops() {
    let p = image_executor();
    assert!(p.supports(&MediaOp::Resize(ResizeOp {
        resolution: Resolution::p720(),
        mode: ResizeMode::Fit,
    })));
    assert!(p.supports(&MediaOp::Crop(CropRegion::new(0, 0, 10, 10))));
    assert!(p.supports(&MediaOp::Rotate(Rotation::Degrees90)));
    assert!(p.supports(&MediaOp::Flip(FlipDirection::Horizontal)));
}

#[test]
fn rejects_video_ops() {
    let p = image_executor();
    assert!(!p.supports(&MediaOp::StripAudio));
    assert!(!p.supports(&MediaOp::StripVideo));
    assert!(!p.supports(&MediaOp::Reverse));
    assert!(!p.supports(&MediaOp::NormalizeAudio));
    assert!(!p.supports(&MediaOp::Speed(2.0)));
}

#[tokio::test]
async fn error_on_unsupported_op() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::StripAudio];
    let result = backend.execute(&source, &ops, None).await;
    assert!(result.is_err(), "should reject unsupported ops");
}

// ── Performance comparison: registered image backend vs raw image crate ───────────────

#[tokio::test]
async fn perf_resize_comparison() {
    let fixture = create_gradient_png(1000, 1000);
    let source = FileSource::from_path(fixture.path());

    // Warm up
    let backend = image_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(200, 200),
        mode: ResizeMode::Exact,
    })];

    // Measure registered image backend
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let _ = backend.execute(&source, &ops, None).await.expect("resize");
    }
    let backend_time = start.elapsed() / 5;

    // Measure raw image crate
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let img = image::open(fixture.path()).expect("open");
        let _resized = img.resize_exact(200, 200, image::imageops::FilterType::Lanczos3);
    }
    let raw_time = start.elapsed() / 5;

    println!("=== Image Resize Performance (1000×1000 → 200×200, avg of 5) ===");
    println!("  registered image backend:   {backend_time:?}");
    println!("  Raw image crate:  {raw_time:?}");
    let overhead_pct = if raw_time.as_nanos() > 0 {
        ((backend_time.as_nanos() as f64 / raw_time.as_nanos() as f64) - 1.0) * 100.0
    } else {
        0.0
    };
    println!("  Overhead:         {overhead_pct:.1}%");

    // registered image backend should not be more than 50% slower than raw
    assert!(
        backend_time < raw_time * 3,
        "registered image backend is too slow: {backend_time:?} vs {raw_time:?}"
    );
}

#[tokio::test]
async fn perf_crop_comparison() {
    let fixture = create_gradient_png(1000, 1000);
    let source = FileSource::from_path(fixture.path());

    let backend = image_executor();
    let ops = vec![MediaOp::Crop(CropRegion::new(100, 100, 500, 500))];

    // Measure registered image backend
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let _ = backend.execute(&source, &ops, None).await.expect("crop");
    }
    let backend_time = start.elapsed() / 5;

    // Measure raw image crate
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let img = image::open(fixture.path()).expect("open");
        let _cropped = img.crop_imm(100, 100, 500, 500);
    }
    let raw_time = start.elapsed() / 5;

    println!("=== Image Crop Performance (1000×1000 → 500×500, avg of 5) ===");
    println!("  registered image backend:   {backend_time:?}");
    println!("  Raw image crate:  {raw_time:?}");
    let overhead_pct = if raw_time.as_nanos() > 0 {
        ((backend_time.as_nanos() as f64 / raw_time.as_nanos() as f64) - 1.0) * 100.0
    } else {
        0.0
    };
    println!("  Overhead:         {overhead_pct:.1}%");
}

#[tokio::test]
async fn perf_multi_op_pipeline() {
    let fixture = create_gradient_png(500, 500);
    let source = FileSource::from_path(fixture.path());

    let backend = image_executor();
    let ops = vec![
        MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(200, 200),
            mode: ResizeMode::Exact,
        }),
        MediaOp::Crop(CropRegion::new(10, 10, 150, 150)),
        MediaOp::Rotate(Rotation::Degrees90),
        MediaOp::Flip(FlipDirection::Horizontal),
    ];

    let start = std::time::Instant::now();
    for _ in 0..5 {
        let _ = backend
            .execute(&source, &ops, None)
            .await
            .expect("multi-op");
    }
    let avg = start.elapsed() / 5;

    println!("=== Multi-Op Pipeline (resize+crop+rotate+flip, avg of 5) ===");
    println!("  Average time: {avg:?}");
}

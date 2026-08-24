//! Integration tests for rskit-media-image with real image fixtures.

use image::{ImageFormat, Rgb, RgbImage};
use rskit_errors::ErrorCode;
use rskit_media::{
    Registry,
    executor::MediaExecutor,
    filter::{Filter, FilterTarget, ParamValue, Params},
    format::Format,
    ops::{CropRegion, FlipDirection, MediaOp, ResizeMode, ResizeOp, Rotation},
    output::OutputConfig,
    probe::MediaProbe,
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

fn create_solid_image(width: u32, height: u32, extension: &str, format: ImageFormat) -> TempFile {
    let img = RgbImage::from_pixel(width, height, Rgb([40, 80, 120]));
    let tmp = TempFile::with_extension(extension).expect("create temp");
    img.save_with_format(tmp.path(), format)
        .expect("save image fixture");
    tmp
}

fn image_executor() -> Arc<dyn MediaExecutor> {
    let mut registry = Registry::default();
    rskit_media_image::register(&mut registry, rskit_media_image::Config::default())
        .expect("register image backend");
    registry.executor("image").expect("image executor")
}

#[test]
fn registering_image_backend_twice_reports_duplicate_executor() {
    let mut registry = Registry::default();
    rskit_media_image::register(&mut registry, rskit_media_image::Config::default())
        .expect("first registration");

    let err = rskit_media_image::register(&mut registry, rskit_media_image::Config::default())
        .expect_err("duplicate image registration must fail");

    assert_eq!(err.code(), ErrorCode::AlreadyExists);
}

fn limited_image_executor(max_pixels: u64) -> Arc<dyn MediaExecutor> {
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_pixels(max_pixels),
    )
    .expect("register image backend");
    registry.executor("image").expect("image executor")
}

fn limited_image_probe(max_pixels: u64) -> Arc<dyn MediaProbe> {
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_pixels(max_pixels),
    )
    .expect("register image backend");
    registry.probe("image").expect("image probe")
}

fn ratio_limited_image_probe(max_decode_ratio: u64) -> Arc<dyn MediaProbe> {
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_decode_ratio(max_decode_ratio),
    )
    .expect("register image backend");
    registry.probe("image").expect("image probe")
}

#[tokio::test]
async fn probe_rejects_source_above_configured_byte_limit() {
    let fixture = create_gradient_png(10, 10);
    let source = FileSource::from_path(fixture.path());
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_source_bytes(1),
    )
    .expect("register image backend");
    let probe = registry.probe("image").expect("image probe");

    let err = probe.probe(&source).await.unwrap_err();

    assert!(err.to_string().contains("max_source_bytes"));
}

#[tokio::test]
async fn probe_rejects_byte_sources_above_configured_byte_limit() {
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_source_bytes(1),
    )
    .expect("register image backend");
    let probe = registry.probe("image").expect("image probe");

    let err = probe
        .probe(&FileSource::Bytes(bytes::Bytes::from_static(b"too large")))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("max_source_bytes"));
}

#[tokio::test]
async fn probe_reports_directory_sources_as_read_errors() {
    let dir = TempDir::new().expect("temp dir");
    let probe = image_probe();

    let err = probe
        .probe(&FileSource::from_path(dir.path()))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.to_string().contains("failed to read image"));
}

#[tokio::test]
async fn probe_rejects_url_sources_before_fetching() {
    let probe = limited_image_probe(100);

    let err = probe
        .probe(&FileSource::from_url("https://example.test/image.png"))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("URL sources not supported"));
}

#[tokio::test]
async fn probe_rejects_invalid_image_bytes() {
    let probe = limited_image_probe(100);

    let err = probe
        .probe(&FileSource::Bytes(bytes::Bytes::from_static(
            b"not an image",
        )))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidFormat);
}

#[tokio::test]
async fn probe_rejects_images_above_decode_ratio_limit() {
    let fixture = create_gradient_png(64, 64);
    let source = FileSource::from_path(fixture.path());
    let probe = ratio_limited_image_probe(1);

    let err = probe.probe(&source).await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("max_decode_ratio"));
}

#[tokio::test]
async fn probe_rejects_images_above_pixel_limit() {
    let fixture = create_gradient_png(11, 10);
    let source = FileSource::from_path(fixture.path());
    let mut registry = Registry::default();
    rskit_media_image::register(
        &mut registry,
        rskit_media_image::Config::default().with_max_pixels(100),
    )
    .expect("register image backend");
    let probe = registry.probe("image").expect("image probe");

    let err = probe.probe(&source).await.unwrap_err();

    assert!(err.to_string().contains("max_pixels"));
}

#[tokio::test]
async fn resize_rejects_output_above_pixel_limit() {
    let fixture = create_gradient_png(10, 10);
    let source = FileSource::from_path(fixture.path());
    let backend = limited_image_executor(100);
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(11, 10),
        mode: ResizeMode::Exact,
    })];

    let err = backend.execute(&source, &ops, None).await.unwrap_err();

    assert!(err.to_string().contains("max_pixels"));
}

#[tokio::test]
async fn resize_fit_rejects_requested_resolution_above_pixel_limit() {
    let fixture = create_gradient_png(10, 10);
    let source = FileSource::from_path(fixture.path());
    let backend = limited_image_executor(100);
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(11, 10),
        mode: ResizeMode::Fit,
    })];

    let err = backend.execute(&source, &ops, None).await.unwrap_err();

    assert!(err.to_string().contains("max_pixels"));
}

#[tokio::test]
async fn thumbnail_rejects_output_above_pixel_limit() {
    let fixture = create_gradient_png(10, 10);
    let source = FileSource::from_path(fixture.path());
    let probe = limited_image_probe(100);

    let err = probe
        .thumbnail(
            &source,
            rskit_media::Timestamp::from_millis(0),
            Some(Resolution::new(11, 10)),
        )
        .await
        .unwrap_err();

    assert!(err.to_string().contains("max_pixels"));
}

#[tokio::test]
async fn thumbnail_default_clamps_short_images_to_one_pixel_height() {
    let fixture = create_gradient_png(1000, 1);
    let source = FileSource::from_path(fixture.path());
    let probe = limited_image_probe(1_000);

    let thumbnail = probe
        .thumbnail(&source, rskit_media::Timestamp::from_millis(0), None)
        .await
        .expect("thumbnail");

    assert_eq!(read_dimensions(&thumbnail), (320, 1));
}

#[tokio::test]
async fn thumbnails_returns_single_resized_image_thumbnail() {
    let fixture = create_gradient_png(40, 20);
    let source = FileSource::from_path(fixture.path());
    let probe = image_probe();

    let thumbnails = probe
        .thumbnails(
            &source,
            std::time::Duration::from_secs(1),
            Some(Resolution::new(10, 10)),
        )
        .await
        .expect("thumbnails");

    assert_eq!(thumbnails.len(), 1);
    assert_eq!(read_dimensions(&thumbnails[0]), (10, 5));
}

#[tokio::test]
async fn probe_reads_managed_temp_sources() {
    let fixture = create_gradient_png(16, 12);
    let source = FileSource::Temp(fixture);
    let probe = image_probe();

    let metadata = probe.probe(&source).await.expect("probe temp image");
    let resolution = metadata.resolution().expect("resolution");

    assert_eq!((resolution.width, resolution.height), (16, 12));
}

fn image_probe() -> Arc<dyn MediaProbe> {
    let mut registry = Registry::default();
    rskit_media_image::register(&mut registry, rskit_media_image::Config::default())
        .expect("register image backend");
    registry.probe("image").expect("image probe")
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
async fn resize_fit_width_clamps_thin_images_to_one_pixel_height() {
    let fixture = create_gradient_png(100, 1);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(1, 0),
        mode: ResizeMode::FitWidth,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fit width");

    assert_eq!(read_dimensions(&result), (1, 1));
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

#[tokio::test]
async fn resize_fit_height_clamps_narrow_images_to_one_pixel_width() {
    let fixture = create_gradient_png(1, 100);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(0, 1),
        mode: ResizeMode::FitHeight,
    })];

    let result = backend
        .execute(&source, &ops, None)
        .await
        .expect("resize fit height");

    assert_eq!(read_dimensions(&result), (1, 1));
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

#[tokio::test]
async fn rotate_arbitrary_degrees_is_rejected() {
    let fixture = create_gradient_png(20, 10);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let err = backend
        .execute(&source, &[MediaOp::Rotate(Rotation::Arbitrary(17.0))], None)
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("arbitrary rotation"));
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

#[tokio::test]
async fn flip_both_preserves_dimensions() {
    let fixture = create_gradient_png(100, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let result = backend
        .execute(&source, &[MediaOp::Flip(FlipDirection::Both)], None)
        .await
        .expect("flip both");

    assert_eq!(read_dimensions(&result), (100, 50));
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

#[tokio::test]
async fn filters_cover_parameter_defaults_and_integer_conversions() {
    let fixture = create_gradient_png(24, 24);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let filters = [
        Filter {
            name: "blur".into(),
            target: FilterTarget::Video,
            params: Params::new().set("radius", ParamValue::Int(2)),
        },
        Filter {
            name: "brightness".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", ParamValue::Float(12.8)),
        },
        Filter {
            name: "contrast".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", ParamValue::Int(2)),
        },
        Filter {
            name: "sharpen".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("sigma", ParamValue::Float(1.0))
                .set("threshold", ParamValue::Int(2)),
        },
        Filter {
            name: "hue".into(),
            target: FilterTarget::Video,
            params: Params::new().set("degrees", ParamValue::Int(90)),
        },
        Filter {
            name: "posterize".into(),
            target: FilterTarget::Video,
            params: Params::new().set("levels", ParamValue::Int(1)),
        },
        Filter {
            name: "pixelate".into(),
            target: FilterTarget::Video,
            params: Params::new().set("size", ParamValue::Int(0)),
        },
    ];

    for filter in filters {
        let result = backend
            .execute(&source, &[MediaOp::Filter(filter)], None)
            .await
            .expect("filter");
        assert_eq!(read_dimensions(&result), (24, 24));
    }

    let extra_filters = [
        Filter {
            name: "sharpen".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("sigma", ParamValue::Int(1))
                .set("threshold", ParamValue::Float(2.0)),
        },
        Filter {
            name: "hue".into(),
            target: FilterTarget::Video,
            params: Params::new().set("degrees", ParamValue::Float(45.0)),
        },
    ];

    for filter in extra_filters {
        let result = backend
            .execute(&source, &[MediaOp::Filter(filter)], None)
            .await
            .expect("extra filter");
        assert_eq!(read_dimensions(&result), (24, 24));
    }
}

#[tokio::test]
async fn filters_ignore_wrong_typed_params_and_use_defaults() {
    let fixture = create_gradient_png(24, 24);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let filters = [
        Filter {
            name: "blur".into(),
            target: FilterTarget::Video,
            params: Params::new().set("radius", ParamValue::Str("wide".into())),
        },
        Filter {
            name: "brightness".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", ParamValue::Bool(true)),
        },
        Filter {
            name: "contrast".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", ParamValue::Str("high".into())),
        },
        Filter {
            name: "sharpen".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("sigma", ParamValue::Str("sharp".into()))
                .set("threshold", ParamValue::Bool(false)),
        },
        Filter {
            name: "hue".into(),
            target: FilterTarget::Video,
            params: Params::new().set("degrees", ParamValue::Str("blue".into())),
        },
        Filter {
            name: "posterize".into(),
            target: FilterTarget::Video,
            params: Params::new().set("levels", ParamValue::Float(3.0)),
        },
        Filter {
            name: "pixelate".into(),
            target: FilterTarget::Video,
            params: Params::new().set("size", ParamValue::Str("large".into())),
        },
    ];

    for filter in filters {
        let result = backend
            .execute(&source, &[MediaOp::Filter(filter)], None)
            .await
            .expect("filter default");
        assert_eq!(read_dimensions(&result), (24, 24));
    }
}

#[tokio::test]
async fn image_filter_rejects_audio_target() {
    let fixture = create_gradient_png(20, 20);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let op = MediaOp::Filter(Filter {
        name: "volume".into(),
        target: FilterTarget::Audio,
        params: Params::new(),
    });

    let err = backend.execute(&source, &[op], None).await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("audio filter"));
}

#[tokio::test]
async fn image_filter_rejects_unknown_filter_names() {
    let fixture = create_gradient_png(20, 20);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let op = MediaOp::Filter(Filter {
        name: "emboss".into(),
        target: FilterTarget::Video,
        params: Params::new(),
    });

    let err = backend.execute(&source, &[op], None).await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("unsupported image filter"));
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

#[tokio::test]
async fn transcode_uses_requested_image_formats_for_memory_output() {
    let cases = [
        ("jpg", "jpg", ImageFormat::Jpeg),
        ("gif", "gif", ImageFormat::Gif),
        ("bmp", "bmp", ImageFormat::Bmp),
        ("tif", "tif", ImageFormat::Tiff),
    ];
    let backend = image_executor();

    for (format_id, extension, expected) in cases {
        let fixture = create_gradient_png(16, 16);
        let source = FileSource::from_path(fixture.path());
        let result = backend
            .execute(
                &source,
                &[MediaOp::Transcode(OutputConfig::new(Format::new(
                    format_id,
                )))],
                Some(&FileSink::Memory),
            )
            .await
            .unwrap_or_else(|err| panic!("transcode {format_id} failed: {err}"));

        let FileSource::Bytes(bytes) = result else {
            panic!("expected memory output");
        };
        let actual = image::guess_format(&bytes)
            .unwrap_or_else(|err| panic!("guess {extension} failed: {err}"));
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn transcode_to_unsupported_format_is_rejected() {
    let backend = image_executor();
    let fixture = create_gradient_png(16, 16);
    let source = FileSource::from_path(fixture.path());

    for format_id in ["made-up", "svg", "heif"] {
        let err = backend
            .execute(
                &source,
                &[MediaOp::Transcode(OutputConfig::new(Format::new(
                    format_id,
                )))],
                Some(&FileSink::Memory),
            )
            .await
            .expect_err("unsupported transcode format must be rejected, not fall back to PNG");
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}

#[tokio::test]
async fn source_extension_selects_output_format_when_not_transcoding() {
    let cases = [
        ("jpeg", ImageFormat::Jpeg),
        ("gif", ImageFormat::Gif),
        ("bmp", ImageFormat::Bmp),
        ("tiff", ImageFormat::Tiff),
        ("webp", ImageFormat::WebP),
        ("bin", ImageFormat::Png),
    ];
    let backend = image_executor();

    for (extension, expected) in cases {
        let source_fixture = create_solid_image(12, 12, extension, expected);
        let source = FileSource::from_path(source_fixture.path());
        let result = backend
            .execute(&source, &[MediaOp::Flip(FlipDirection::Horizontal)], None)
            .await
            .unwrap_or_else(|err| panic!("execute for {extension} failed: {err}"));
        let FileSource::Bytes(bytes) = result else {
            panic!("expected memory output");
        };
        let actual = image::guess_format(&bytes)
            .unwrap_or_else(|err| panic!("guess for {extension} failed: {err}"));
        assert_eq!(actual, expected);
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
async fn output_to_path_reports_parent_directory_creation_errors() {
    let fixture = create_gradient_png(20, 20);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let dir = TempDir::new().expect("temp dir");
    let file_parent = dir.path().join("not-a-dir");
    std::fs::write(&file_parent, b"file").expect("write parent placeholder");
    let sink = FileSink::Path(file_parent.join("result.png"));

    let err = backend
        .execute(
            &source,
            &[MediaOp::Flip(FlipDirection::Horizontal)],
            Some(&sink),
        )
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.to_string().contains("create dir failed"));
}

#[tokio::test]
async fn output_to_path_reports_write_errors() {
    let fixture = create_gradient_png(20, 20);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let dir = TempDir::new().expect("temp dir");
    let sink = FileSink::Path(dir.path().to_path_buf());

    let err = backend
        .execute(
            &source,
            &[MediaOp::Flip(FlipDirection::Horizontal)],
            Some(&sink),
        )
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.to_string().contains("write image failed"));
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

#[tokio::test]
async fn execute_with_progress_delegates_to_execute() {
    let fixture = create_gradient_png(50, 50);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let result = backend
        .execute_with_progress(
            &source,
            &[MediaOp::Flip(FlipDirection::Horizontal)],
            Some(&FileSink::Memory),
            Box::new(|_| {}),
        )
        .await
        .expect("execute with progress");

    assert_eq!(read_dimensions(&result), (50, 50));
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

#[test]
fn preview_reports_operation_count() {
    let p = image_executor();

    let preview = p
        .preview(
            &FileSource::Bytes(bytes::Bytes::new()),
            &[
                MediaOp::Flip(FlipDirection::Horizontal),
                MediaOp::Flip(FlipDirection::Vertical),
            ],
        )
        .expect("preview");

    assert_eq!(preview, vec!["ImageProcessor: 2 operations"]);
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

    let backend = image_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(200, 200),
        mode: ResizeMode::Exact,
    })];

    // Warm up both paths so one-time costs don't land in a measured iteration.
    let _ = backend.execute(&source, &ops, None).await.expect("resize");
    let img = image::open(fixture.path()).expect("open");
    let _ = img.resize_exact(200, 200, image::imageops::FilterType::Lanczos3);

    // Interleave both paths and keep the best per-iteration time of each.
    // Averages of sequential loops are unreliable under parallel test load:
    // neighboring test processes inflate whichever loop happens to run during
    // a contention spike. The minimum discards those spikes.
    const RUNS: u32 = 10;
    let mut backend_time = std::time::Duration::MAX;
    let mut raw_time = std::time::Duration::MAX;
    for _ in 0..RUNS {
        let start = std::time::Instant::now();
        let _ = backend.execute(&source, &ops, None).await.expect("resize");
        backend_time = backend_time.min(start.elapsed());

        let start = std::time::Instant::now();
        let img = image::open(fixture.path()).expect("open");
        let _resized = img.resize_exact(200, 200, image::imageops::FilterType::Lanczos3);
        raw_time = raw_time.min(start.elapsed());
    }

    println!("=== Image Resize Performance (1000×1000 → 200×200, best of {RUNS}) ===");
    println!("  registered image backend:   {backend_time:?}");
    println!("  Raw image crate:  {raw_time:?}");
    let overhead_pct = if raw_time.as_nanos() > 0 {
        ((backend_time.as_nanos() as f64 / raw_time.as_nanos() as f64) - 1.0) * 100.0
    } else {
        0.0
    };
    println!("  Overhead:         {overhead_pct:.1}%");

    // registered image backend should stay well under 3x the raw crate even on
    // a heavily loaded machine
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

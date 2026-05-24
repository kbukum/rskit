//! Criterion benchmarks comparing the registered image backend vs raw `image` crate.
//!
//! Run:  cargo bench -p rskit-media-image
//! Report: target/criterion/report/index.html

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use image::{Rgb, RgbImage, imageops};
use rskit_media::{
    Registry,
    executor::MediaExecutor,
    ops::{CropRegion, FlipDirection, MediaOp, ResizeMode, ResizeOp, Rotation},
    spatial::Resolution,
};
use rskit_storage::{FileSource, TempFile};
use std::sync::Arc;

/// Create a gradient test image at given dimensions.
fn create_fixture(width: u32, height: u32) -> TempFile {
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width) as u8;
            let g = (y * 255 / height) as u8;
            img.put_pixel(x, y, Rgb([r, g, 128]));
        }

        fn image_executor() -> Arc<dyn MediaExecutor> {
            let mut registry = Registry::default();
            rskit_media_image::register(&mut registry, rskit_media_image::Config)
                .expect("register image backend");
            registry.executor("image").expect("image executor")
        }
    }
    let tmp = TempFile::with_extension("png").expect("create temp");
    img.save(tmp.path()).expect("save");
    tmp
}

fn bench_resize(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fixture = create_fixture(1000, 1000);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(200, 200),
        mode: ResizeMode::Exact,
    })];

    let mut group = c.benchmark_group("resize_1000_to_200");

    group.bench_function("registered_image_backend", |b| {
        b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
    });

    group.bench_function("raw_image_crate", |b| {
        b.iter(|| {
            let img = image::open(fixture.path()).unwrap();
            img.resize_exact(200, 200, imageops::FilterType::Lanczos3)
        })
    });

    group.finish();
}

fn bench_resize_sizes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let backend = image_executor();

    let mut group = c.benchmark_group("resize_various_sizes");
    group.sample_size(10);

    for &(src_size, dst_size) in &[(500, 100), (1000, 200), (2000, 400)] {
        let fixture = create_fixture(src_size, src_size);
        let source = FileSource::from_path(fixture.path());
        let ops = vec![MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(dst_size, dst_size),
            mode: ResizeMode::Exact,
        })];

        group.bench_with_input(
            BenchmarkId::new("registered_image_backend", format!("{src_size}→{dst_size}")),
            &(src_size, dst_size),
            |b, _| {
                b.iter(|| {
                    rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() })
                })
            },
        );

        let fixture_path = fixture.path().to_owned();
        group.bench_with_input(
            BenchmarkId::new("raw_image_crate", format!("{src_size}→{dst_size}")),
            &(src_size, dst_size),
            |b, &(_, dst)| {
                b.iter(|| {
                    let img = image::open(&fixture_path).unwrap();
                    img.resize_exact(dst, dst, imageops::FilterType::Lanczos3)
                })
            },
        );
    }

    group.finish();
}

fn bench_crop(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fixture = create_fixture(1000, 1000);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let ops = vec![MediaOp::Crop(CropRegion::new(100, 100, 500, 500))];

    let mut group = c.benchmark_group("crop_1000_to_500");

    group.bench_function("registered_image_backend", |b| {
        b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
    });

    group.bench_function("raw_image_crate", |b| {
        b.iter(|| {
            let img = image::open(fixture.path()).unwrap();
            img.crop_imm(100, 100, 500, 500)
        })
    });

    group.finish();
}

fn bench_rotate(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fixture = create_fixture(500, 500);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();
    let ops = vec![MediaOp::Rotate(Rotation::Degrees90)];

    let mut group = c.benchmark_group("rotate90_500");

    group.bench_function("registered_image_backend", |b| {
        b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
    });

    group.bench_function("raw_image_crate", |b| {
        b.iter(|| {
            let img = image::open(fixture.path()).unwrap();
            img.rotate90()
        })
    });

    group.finish();
}

fn bench_pipeline(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fixture = create_fixture(500, 500);
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

    let mut group = c.benchmark_group("pipeline_4ops");

    group.bench_function("registered_image_backend", |b| {
        b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
    });

    group.bench_function("raw_image_crate", |b| {
        b.iter(|| {
            let img = image::open(fixture.path()).unwrap();
            let img = img.resize_exact(200, 200, imageops::FilterType::Lanczos3);
            let img = img.crop_imm(10, 10, 150, 150);
            let img = img.rotate90();
            img.fliph()
        })
    });

    group.finish();
}

fn bench_blur(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fixture = create_fixture(500, 500);
    let source = FileSource::from_path(fixture.path());
    let backend = image_executor();

    let ops = vec![MediaOp::Filter(rskit_media::filter::Filter {
        name: "blur".into(),
        target: rskit_media::filter::FilterTarget::Video,
        params: rskit_media::filter::Params::new()
            .set("radius", rskit_media::filter::ParamValue::Float(3.0)),
    })];

    let mut group = c.benchmark_group("blur_500");

    group.bench_function("registered_image_backend", |b| {
        b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
    });

    group.bench_function("raw_image_crate", |b| {
        b.iter(|| {
            let img = image::open(fixture.path()).unwrap();
            img.blur(3.0)
        })
    });

    group.finish();
}

fn real_fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/image")
        .join(name)
}

fn bench_real_fixtures(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let backend = image_executor();
    let ops = vec![MediaOp::Resize(ResizeOp {
        resolution: Resolution::new(200, 200),
        mode: ResizeMode::Exact,
    })];

    let mut group = c.benchmark_group("real_fixture_resize");
    group.sample_size(10);

    let jpeg_path = real_fixture_path("real-photo.jpg");
    let png_path = real_fixture_path("sample.png");

    if jpeg_path.exists() {
        let source = FileSource::from_path(&jpeg_path);
        group.bench_function("real_jpeg_500x378", |b| {
            b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
        });
    }

    if png_path.exists() {
        let source = FileSource::from_path(&png_path);
        group.bench_function("real_png_600x600", |b| {
            b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
        });
    }

    group.finish();
}

fn bench_real_fixture_pipeline(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
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

    let mut group = c.benchmark_group("real_fixture_pipeline");
    group.sample_size(10);

    let jpeg_path = real_fixture_path("real-photo.jpg");
    let png_path = real_fixture_path("sample.png");

    if jpeg_path.exists() {
        let source = FileSource::from_path(&jpeg_path);
        group.bench_function("real_jpeg_pipeline", |b| {
            b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
        });
    }

    if png_path.exists() {
        let source = FileSource::from_path(&png_path);
        group.bench_function("real_png_pipeline", |b| {
            b.iter(|| rt.block_on(async { backend.execute(&source, &ops, None).await.unwrap() }))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_resize,
    bench_resize_sizes,
    bench_crop,
    bench_rotate,
    bench_pipeline,
    bench_blur,
    bench_real_fixtures,
    bench_real_fixture_pipeline,
);
criterion_main!(benches);

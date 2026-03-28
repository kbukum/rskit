//! Image processing executor using the `image` crate.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, imageops};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSink, FileSource, TempFile};
use rskit_media::{
    executor::MediaExecutor,
    filter::FilterTarget,
    ops::*,
    pipeline::Progress,
};

/// Image-specific executor using the `image` crate.
///
/// Handles image operations (Resize, Crop, Rotate, Flip, subset of Filters,
/// Transcode). Returns `Err(unsupported)` for video/audio operations.
pub struct ImageProcessor;

impl ImageProcessor {
    /// Create a new image processor.
    pub fn new() -> Self {
        Self
    }

    fn load_image(source: &FileSource) -> AppResult<DynamicImage> {
        let data = match source {
            FileSource::Path(p) => std::fs::read(p).map_err(|e| {
                AppError::new(ErrorCode::NotFound, format!("failed to read image: {e}"))
            })?,
            FileSource::Bytes(b) => b.to_vec(),
            FileSource::Temp(t) => std::fs::read(t.path()).map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to read temp image: {e}"))
            })?,
            FileSource::Url(_) => {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "URL sources not supported; use to_local_path() first",
                ))
            }
        };

        image::load_from_memory(&data).map_err(|e| {
            AppError::new(ErrorCode::InvalidFormat, format!("failed to decode image: {e}"))
        })
    }

    fn save_image(img: &DynamicImage, format: ImageFormat) -> AppResult<Vec<u8>> {
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        img.write_to(&mut cursor, format).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to encode image: {e}"))
        })?;
        Ok(buf)
    }

    fn detect_format(source: &FileSource) -> ImageFormat {
        let ext = source.extension().unwrap_or("png");
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            "gif" => ImageFormat::Gif,
            "bmp" => ImageFormat::Bmp,
            "tiff" | "tif" => ImageFormat::Tiff,
            "webp" => ImageFormat::WebP,
            "avif" => ImageFormat::Avif,
            _ => ImageFormat::Png,
        }
    }

    fn format_from_name(name: &str) -> ImageFormat {
        match name {
            "jpeg" | "jpg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            "gif" => ImageFormat::Gif,
            "bmp" => ImageFormat::Bmp,
            "tiff" | "tif" => ImageFormat::Tiff,
            "webp" => ImageFormat::WebP,
            "avif" => ImageFormat::Avif,
            _ => ImageFormat::Png,
        }
    }
}

impl Default for ImageProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl MediaExecutor for ImageProcessor {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> AppResult<FileSource> {
        let mut img = Self::load_image(source)?;
        let mut output_format = Self::detect_format(source);

        for op in ops {
            match op {
                MediaOp::Resize(resize_op) => {
                    let (w, h) = (resize_op.resolution.width, resize_op.resolution.height);
                    img = match resize_op.mode {
                        ResizeMode::Exact => {
                            img.resize_exact(w, h, imageops::FilterType::Lanczos3)
                        }
                        ResizeMode::Fit => {
                            img.resize(w, h, imageops::FilterType::Lanczos3)
                        }
                        ResizeMode::Fill => {
                            let resized = img.resize_to_fill(w, h, imageops::FilterType::Lanczos3);
                            resized
                        }
                        ResizeMode::FitWidth => {
                            let ratio = w as f64 / img.width() as f64;
                            let new_h = (img.height() as f64 * ratio) as u32;
                            img.resize_exact(w, new_h, imageops::FilterType::Lanczos3)
                        }
                        ResizeMode::FitHeight => {
                            let ratio = h as f64 / img.height() as f64;
                            let new_w = (img.width() as f64 * ratio) as u32;
                            img.resize_exact(new_w, h, imageops::FilterType::Lanczos3)
                        }
                    };
                }
                MediaOp::Crop(region) => {
                    img = img.crop_imm(region.x, region.y, region.width, region.height);
                }
                MediaOp::Rotate(rotation) => {
                    img = match rotation {
                        Rotation::Degrees90 => img.rotate90(),
                        Rotation::Degrees180 => img.rotate180(),
                        Rotation::Degrees270 => img.rotate270(),
                        Rotation::Arbitrary(_) => {
                            // image crate only supports 90° increments natively
                            img
                        }
                    };
                }
                MediaOp::Flip(dir) => {
                    img = match dir {
                        FlipDirection::Horizontal => img.fliph(),
                        FlipDirection::Vertical => img.flipv(),
                        FlipDirection::Both => img.fliph().flipv(),
                    };
                }
                MediaOp::Filter(filter) => {
                    if filter.target == FilterTarget::Video {
                        match filter.name.as_str() {
                            "grayscale" => {
                                img = DynamicImage::ImageLuma8(img.to_luma8());
                            }
                            "blur" => {
                                let radius = filter
                                    .params
                                    .get("radius")
                                    .and_then(|v| match v {
                                        rskit_media::filter::ParamValue::Float(f) => Some(*f as f32),
                                        rskit_media::filter::ParamValue::Int(i) => Some(*i as f32),
                                        _ => None,
                                    })
                                    .unwrap_or(1.0);
                                img = img.blur(radius);
                            }
                            "brightness" => {
                                let value = filter
                                    .params
                                    .get("value")
                                    .and_then(|v| match v {
                                        rskit_media::filter::ParamValue::Float(f) => Some(*f as i32),
                                        rskit_media::filter::ParamValue::Int(i) => Some(*i as i32),
                                        _ => None,
                                    })
                                    .unwrap_or(0);
                                img = img.brighten(value);
                            }
                            "contrast" => {
                                let value = filter
                                    .params
                                    .get("value")
                                    .and_then(|v| match v {
                                        rskit_media::filter::ParamValue::Float(f) => Some(*f as f32),
                                        rskit_media::filter::ParamValue::Int(i) => Some(*i as f32),
                                        _ => None,
                                    })
                                    .unwrap_or(0.0);
                                img = img.adjust_contrast(value);
                            }
                            _ => {
                                tracing::warn!(filter = %filter.name, "unsupported image filter, skipping");
                            }
                        }
                    }
                }
                MediaOp::Transcode(config) => {
                    let format_id = config.format.id();
                    output_format = Self::format_from_name(format_id);
                }
                _ => {
                    if !self.supports(op) {
                        return Err(AppError::new(
                            ErrorCode::InvalidInput,
                            format!("image processor does not support operation: {op:?}"),
                        ));
                    }
                }
            }
        }

        let data = Self::save_image(&img, output_format)?;

        match sink {
            Some(FileSink::Path(p)) => {
                if let Some(parent) = p.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        AppError::new(ErrorCode::Internal, format!("create dir failed: {e}"))
                    })?;
                }
                tokio::fs::write(p, &data).await.map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("write image failed: {e}"))
                })?;
                Ok(FileSource::Path(p.clone()))
            }
            Some(FileSink::Memory) | None => Ok(FileSource::Bytes(bytes::Bytes::from(data))),
            Some(FileSink::Temp) => {
                let ext = match output_format {
                    ImageFormat::Jpeg => "jpg",
                    ImageFormat::Png => "png",
                    ImageFormat::Gif => "gif",
                    ImageFormat::Bmp => "bmp",
                    ImageFormat::Tiff => "tiff",
                    ImageFormat::WebP => "webp",
                    _ => "png",
                };
                let tmp = TempFile::with_extension(ext)?;
                tokio::fs::write(tmp.path(), &data).await.map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("write temp image failed: {e}"))
                })?;
                Ok(tmp.into_source())
            }
        }
    }

    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        _on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> AppResult<FileSource> {
        // Image processing is typically fast enough that progress isn't needed
        self.execute(source, ops, sink).await
    }

    fn supports(&self, op: &MediaOp) -> bool {
        matches!(
            op,
            MediaOp::Resize(_)
                | MediaOp::Crop(_)
                | MediaOp::Rotate(_)
                | MediaOp::Flip(_)
                | MediaOp::Filter(_)
                | MediaOp::Transcode(_)
        )
    }

    fn preview(&self, _source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>> {
        Ok(vec![format!("ImageProcessor: {} operations", ops.len())])
    }
}

//! Image processing executor using the `image` crate.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, imageops};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{executor::MediaExecutor, filter::FilterTarget, ops::*, pipeline::Progress};
use rskit_storage::{FileSink, FileSource, TempFile};

/// Image-specific executor using the `image` crate.
///
/// Handles image operations (Resize, Crop, Rotate, Flip, subset of Filters,
/// Transcode). Returns `Err(unsupported)` for video/audio operations.
pub(crate) struct ImageProcessor;

impl ImageProcessor {
    /// Create a new image processor.
    pub(crate) fn new() -> Self {
        Self
    }

    fn load_image(source: &FileSource) -> AppResult<DynamicImage> {
        let data = match source {
            FileSource::Path(p) => std::fs::read(p).map_err(|e| {
                AppError::new(ErrorCode::NotFound, format!("failed to read image: {e}"))
            })?,
            FileSource::Bytes(b) => b.to_vec(),
            FileSource::Temp(t) => std::fs::read(t.path()).map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to read temp image: {e}"),
                )
            })?,
            FileSource::Url(_) => {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "URL sources not supported; use to_local_path() first",
                ));
            }
        };

        image::load_from_memory(&data).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("failed to decode image: {e}"),
            )
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
                        ResizeMode::Exact => img.resize_exact(w, h, imageops::FilterType::Lanczos3),
                        ResizeMode::Fit => img.resize(w, h, imageops::FilterType::Lanczos3),
                        ResizeMode::Fill => {
                            img.resize_to_fill(w, h, imageops::FilterType::Lanczos3)
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
                        Rotation::Arbitrary(degrees) => {
                            return Err(AppError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "arbitrary rotation ({degrees}°) is not supported by the \
                                     image backend; use 90°/180°/270° or the FFmpeg backend"
                                ),
                            ));
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
                    if filter.target != FilterTarget::Video {
                        return Err(AppError::new(
                            ErrorCode::InvalidInput,
                            format!(
                                "audio filter \"{}\" cannot be applied to images",
                                filter.name
                            ),
                        ));
                    }
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
                        "sharpen" => {
                            let sigma = filter
                                .params
                                .get("sigma")
                                .and_then(|v| match v {
                                    rskit_media::filter::ParamValue::Float(f) => Some(*f as f32),
                                    rskit_media::filter::ParamValue::Int(i) => Some(*i as f32),
                                    _ => None,
                                })
                                .unwrap_or(1.0);
                            let threshold = filter
                                .params
                                .get("threshold")
                                .and_then(|v| match v {
                                    rskit_media::filter::ParamValue::Int(i) => Some(*i),
                                    rskit_media::filter::ParamValue::Float(f) => Some(*f as i64),
                                    _ => None,
                                })
                                .unwrap_or(1);
                            img = img.unsharpen(sigma, threshold as i32);
                        }
                        "hue" => {
                            let degrees = filter
                                .params
                                .get("degrees")
                                .and_then(|v| match v {
                                    rskit_media::filter::ParamValue::Int(i) => Some(*i as i32),
                                    rskit_media::filter::ParamValue::Float(f) => Some(*f as i32),
                                    _ => None,
                                })
                                .unwrap_or(0);
                            img = img.huerotate(degrees);
                        }
                        "invert" => {
                            img.invert();
                        }
                        "sepia" => {
                            // Sepia tone: convert to RGB, apply matrix
                            let mut rgba = img.to_rgba8();
                            for pixel in rgba.pixels_mut() {
                                let [r, g, b, a] = pixel.0;
                                let rf = r as f64;
                                let gf = g as f64;
                                let bf = b as f64;
                                let new_r = (0.393 * rf + 0.769 * gf + 0.189 * bf).min(255.0) as u8;
                                let new_g = (0.349 * rf + 0.686 * gf + 0.168 * bf).min(255.0) as u8;
                                let new_b = (0.272 * rf + 0.534 * gf + 0.131 * bf).min(255.0) as u8;
                                pixel.0 = [new_r, new_g, new_b, a];
                            }
                            img = DynamicImage::ImageRgba8(rgba);
                        }
                        "posterize" => {
                            let levels = filter
                                .params
                                .get("levels")
                                .and_then(|v| match v {
                                    rskit_media::filter::ParamValue::Int(i) => {
                                        Some((*i as u8).max(2))
                                    }
                                    _ => None,
                                })
                                .unwrap_or(4);
                            let step = 255u16 / (levels as u16 - 1);
                            let mut rgba = img.to_rgba8();
                            for pixel in rgba.pixels_mut() {
                                for c in 0..3 {
                                    let val = pixel.0[c] as u16;
                                    let quantized =
                                        ((val * (levels as u16 - 1) + 127) / 255) * step;
                                    pixel.0[c] = quantized.min(255) as u8;
                                }
                            }
                            img = DynamicImage::ImageRgba8(rgba);
                        }
                        "pixelate" => {
                            let block_size = filter
                                .params
                                .get("size")
                                .and_then(|v| match v {
                                    rskit_media::filter::ParamValue::Int(i) => {
                                        Some((*i as u32).max(1))
                                    }
                                    _ => None,
                                })
                                .unwrap_or(8);
                            let (w, h) = (img.width(), img.height());
                            let small_w = (w / block_size).max(1);
                            let small_h = (h / block_size).max(1);
                            img = img
                                .resize_exact(small_w, small_h, imageops::FilterType::Nearest)
                                .resize_exact(w, h, imageops::FilterType::Nearest);
                        }
                        other => {
                            return Err(AppError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "unsupported image filter: \"{other}\"; supported \
                                         filters: grayscale, blur, brightness, contrast, \
                                         sharpen, hue, invert, sepia, posterize, pixelate"
                                ),
                            ));
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

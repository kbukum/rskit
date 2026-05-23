//! Transform trait and built-in transforms.

use crate::{DataItem, DatasetLimits};
use rskit_errors::AppResult;

#[cfg(feature = "image-transform")]
use crate::DataPayload;
#[cfg(feature = "image-transform")]
use rskit_errors::{AppError, ErrorCode};
#[cfg(feature = "image-transform")]
use rskit_validation::Validator;

/// Protocol for data transforms.
pub trait Transform: Send + Sync {
    /// Stable transform name.
    fn name(&self) -> &str;
    /// Apply the transform, returning `Ok(None)` when the item is filtered out.
    fn apply(&self, item: DataItem, limits: &DatasetLimits) -> AppResult<Option<DataItem>>;
}

/// Resize images to a fixed size and re-encode as JPEG.
#[cfg(feature = "image-transform")]
#[derive(Clone)]
pub struct ResizeTransform {
    width: u32,
    height: u32,
    quality: u8,
    name: String,
}

#[cfg(feature = "image-transform")]
impl ResizeTransform {
    /// Create a resize transform after validating dimensions.
    pub fn new(width: u32, height: u32) -> AppResult<Self> {
        Validator::new()
            .min_value("width", width, 1)
            .min_value("height", height, 1)
            .validate()?;
        Ok(Self {
            width,
            height,
            quality: 85,
            name: format!("resize-{width}x{height}"),
        })
    }

    /// Set JPEG quality after validating the accepted encoder range.
    #[must_use = "builder methods return the updated transform"]
    pub fn with_quality(mut self, quality: u8) -> AppResult<Self> {
        Validator::new()
            .in_range("quality", quality, 1, 100)
            .validate()?;
        self.quality = quality;
        Ok(self)
    }
}

#[cfg(feature = "image-transform")]
impl Transform for ResizeTransform {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply(&self, item: DataItem, limits: &DatasetLimits) -> AppResult<Option<DataItem>> {
        use image::ImageReader;
        use std::io::Cursor;

        let bytes = item.payload().read_bytes_bounded(limits)?;
        let img = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("image format detection failed: {error}"),
                )
            })?
            .decode()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("image decode failed: {error}"),
                )
            })?;

        let resized = img.resize_exact(
            self.width,
            self.height,
            image::imageops::FilterType::Lanczos3,
        );

        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, self.quality)
            .encode_image(&resized)
            .map_err(|error| {
                AppError::new(ErrorCode::Internal, format!("image encode failed: {error}"))
            })?;

        Ok(Some(
            item.try_with_payload(DataPayload::bytes(buf, limits)?, limits)?
                .with_extension(".jpg"),
        ))
    }
}

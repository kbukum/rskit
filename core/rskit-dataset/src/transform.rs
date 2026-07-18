//! Transform trait and built-in transforms.

use crate::{DatasetItem, DatasetLimits};
use rskit_errors::AppResult;

#[cfg(feature = "image-transform")]
use crate::DataItem;
#[cfg(feature = "image-transform")]
use crate::DataPayload;
#[cfg(feature = "image-transform")]
use rskit_errors::{AppError, ErrorCode};
#[cfg(feature = "image-transform")]
use rskit_validation::Validator;

/// Protocol for data transforms from item type `I` to item type `O`.
pub trait Transform<I: DatasetItem, O: DatasetItem>: Send + Sync {
    /// Stable transform name.
    fn name(&self) -> &str;
    /// Apply the transform, returning `Ok(None)` when the item is filtered out.
    fn apply(&self, item: I, limits: &DatasetLimits) -> AppResult<Option<O>>;
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
impl Transform<DataItem, DataItem> for ResizeTransform {
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

#[cfg(all(test, feature = "image-transform"))]
mod tests {
    use image::{ImageBuffer, ImageFormat, Rgb};
    use std::io::Cursor;

    use super::*;
    use crate::{Label, MediaType};

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_pixel(width, height, Rgb([10_u8, 20, 30]));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn resize_transform_validates_dimensions_and_quality() {
        assert!(ResizeTransform::new(0, 10).is_err());
        assert!(ResizeTransform::new(10, 0).is_err());

        let transform = ResizeTransform::new(4, 5).unwrap();
        assert_eq!(transform.name(), "resize-4x5");
        assert!(transform.clone().with_quality(0).is_err());
        assert!(transform.clone().with_quality(101).is_err());
        assert_eq!(transform.with_quality(90).unwrap().quality, 90);
    }

    #[test]
    fn resize_transform_reencodes_image_and_preserves_metadata() {
        let limits = DatasetLimits::default();
        let item = DataItem::new(png_bytes(8, 6), Label::Real, MediaType::Image, "fixture")
            .unwrap()
            .with_extension(".png")
            .with_source_offset(42);
        let transform = ResizeTransform::new(3, 2)
            .unwrap()
            .with_quality(80)
            .unwrap();

        let output = transform.apply(item, &limits).unwrap().unwrap();
        let bytes = output.payload().read_bytes_bounded(&limits).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();

        assert_eq!(decoded.width(), 3);
        assert_eq!(decoded.height(), 2);
        assert_eq!(output.extension, ".jpg");
        assert_eq!(output.source_offset(), Some(42));
        assert_eq!(output.source_name, "fixture");
    }

    #[test]
    fn resize_transform_rejects_non_image_payloads() {
        let item = DataItem::new(
            b"not an image".to_vec(),
            Label::AiGenerated,
            MediaType::Image,
            "bad",
        )
        .unwrap();

        let error = ResizeTransform::new(2, 2)
            .unwrap()
            .apply(item, &DatasetLimits::default())
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}

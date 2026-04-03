//! Transform trait and built-in transforms.

use crate::DataItem;

/// Protocol for data transforms.
pub trait Transform: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, item: DataItem) -> Option<DataItem>;
}

/// Resize images to a fixed size and re-encode as JPEG.
#[cfg(feature = "image-transform")]
pub struct ResizeTransform {
    width: u32,
    height: u32,
    quality: u8,
    name: String,
}

#[cfg(feature = "image-transform")]
impl ResizeTransform {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            quality: 85,
            name: format!("resize-{width}x{height}"),
        }
    }

    pub fn with_quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }
}

#[cfg(feature = "image-transform")]
impl Transform for ResizeTransform {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply(&self, item: DataItem) -> Option<DataItem> {
        use image::ImageReader;
        use std::io::Cursor;

        let img = ImageReader::new(Cursor::new(&item.content))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;

        let resized = img.resize_exact(
            self.width,
            self.height,
            image::imageops::FilterType::Lanczos3,
        );

        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        resized
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .ok()?;

        Some(DataItem {
            content: buf,
            extension: ".jpg".to_string(),
            ..item
        })
    }
}

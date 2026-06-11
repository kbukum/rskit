//! Bounded image source loading and decoding.
//!
//! The image adapter intentionally centralizes all read/decode limits here so
//! probe and executor paths cannot drift in how they handle untrusted media.

use std::io::{Cursor, ErrorKind, Read as _};

use image::{DynamicImage, ImageFormat, ImageReader};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::spatial::Resolution;
use rskit_storage::FileSource;

use crate::config::Config;

pub(crate) struct DecodedImage {
    pub(crate) image: DynamicImage,
    pub(crate) format: Option<ImageFormat>,
}

pub(crate) fn load_data(source: &FileSource, config: &Config) -> AppResult<Vec<u8>> {
    match source {
        FileSource::Path(path) => read_path_bounded(path, config.max_source_bytes),
        FileSource::Bytes(bytes) => {
            ensure_source_len(bytes.len() as u64, config)?;
            Ok(bytes.to_vec())
        }
        FileSource::Temp(temp) => read_path_bounded(temp.path(), config.max_source_bytes),
        FileSource::Url(_) => Err(AppError::new(
            ErrorCode::InvalidInput,
            "URL sources not supported; use to_local_path() first",
        )),
    }
}

pub(crate) fn decode_image(data: &[u8], config: &Config) -> AppResult<DecodedImage> {
    ensure_source_len(data.len() as u64, config)?;
    let reader = reader_for(data, config)?;
    let format = reader.format();
    let (width, height) = dimensions_for(data, config)?;
    ensure_dimensions(width, height, config)?;

    let image = reader.decode().map_err(|error| {
        AppError::new(
            ErrorCode::InvalidFormat,
            format!("failed to decode image: {error}"),
        )
    })?;
    ensure_decoded_ratio(&image, data.len() as u64, config)?;
    ensure_image_dimensions(&image, config)?;
    Ok(DecodedImage { image, format })
}

pub(crate) fn dimensions_for(data: &[u8], config: &Config) -> AppResult<(u32, u32)> {
    let reader = reader_for(data, config)?;
    reader.into_dimensions().map_err(|error| {
        AppError::new(
            ErrorCode::InvalidFormat,
            format!("failed to read image dimensions: {error}"),
        )
    })
}

fn reader_for<'a>(data: &'a [u8], config: &Config) -> AppResult<ImageReader<Cursor<&'a [u8]>>> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("failed to detect image format: {error}"),
            )
        })?;
    reader.limits(config.image_limits());
    Ok(reader)
}

fn read_path_bounded(path: &std::path::Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        AppError::new(
            open_error_code(error.kind()),
            format!("failed to open image {}: {error}", path.display()),
        )
    })?;
    let mut data = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read image {}: {error}", path.display()),
            )
        })?;
    if data.len() as u64 > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "image source {} exceeds max_source_bytes={max_bytes}",
                path.display()
            ),
        ));
    }
    Ok(data)
}

fn open_error_code(kind: ErrorKind) -> ErrorCode {
    match kind {
        ErrorKind::NotFound => ErrorCode::NotFound,
        _ => ErrorCode::Internal,
    }
}

fn ensure_source_len(len: u64, config: &Config) -> AppResult<()> {
    if len > config.max_source_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "image source is {len} bytes, exceeding max_source_bytes={}",
                config.max_source_bytes
            ),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_resolution(resolution: Resolution, config: &Config) -> AppResult<()> {
    ensure_dimensions(resolution.width, resolution.height, config)
}

pub(crate) fn ensure_image_dimensions(image: &DynamicImage, config: &Config) -> AppResult<()> {
    ensure_dimensions(image.width(), image.height(), config)
}

pub(crate) fn scaled_dimension(source: u32, target: u32, reference: u32) -> u32 {
    if reference == 0 {
        return 1;
    }
    ((f64::from(source) * f64::from(target)) / f64::from(reference))
        .round()
        .max(1.0) as u32
}

fn ensure_dimensions(width: u32, height: u32, config: &Config) -> AppResult<()> {
    if width == 0 || height == 0 {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("image dimensions must be non-zero, got {width}x{height}"),
        ));
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > config.max_pixels {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "image dimensions {width}x{height} contain {pixels} pixels, exceeding max_pixels={}",
                config.max_pixels
            ),
        ));
    }
    Ok(())
}

fn ensure_decoded_ratio(
    image: &DynamicImage,
    compressed_bytes: u64,
    config: &Config,
) -> AppResult<()> {
    let decoded_bytes = u64::from(image.width())
        .saturating_mul(u64::from(image.height()))
        .saturating_mul(u64::from(image.color().bytes_per_pixel()));
    let allowed = compressed_bytes
        .max(1)
        .saturating_mul(config.max_decode_ratio.max(1));
    if decoded_bytes > allowed {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "decoded image expands to {decoded_bytes} bytes from {compressed_bytes} bytes, \
                 exceeding max_decode_ratio={}",
                config.max_decode_ratio
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rskit_errors::ErrorCode;
    use rskit_media::spatial::Resolution;

    use super::{ensure_resolution, read_path_bounded, scaled_dimension};

    #[test]
    fn read_path_bounded_reports_missing_paths_as_not_found() {
        let dir = rskit_storage::TempDir::new().unwrap();

        let error = read_path_bounded(&dir.path().join("missing.png"), 1024).unwrap_err();

        assert_eq!(error.code(), ErrorCode::NotFound);
    }

    #[test]
    fn read_path_bounded_reports_non_not_found_open_errors_as_internal() {
        let dir = rskit_storage::TempDir::new().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, b"not a dir").unwrap();

        let error = read_path_bounded(&file.join(Path::new("child.png")), 1024).unwrap_err();

        assert_eq!(error.code(), ErrorCode::Internal);
    }

    #[test]
    fn ensure_resolution_rejects_zero_width() {
        let error =
            ensure_resolution(Resolution::new(0, 10), &crate::Config::default()).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn ensure_resolution_rejects_zero_height() {
        let error =
            ensure_resolution(Resolution::new(10, 0), &crate::Config::default()).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn scaled_dimension_rounds_and_clamps_to_one() {
        assert_eq!(scaled_dimension(10, 10, 0), 1);
        assert_eq!(scaled_dimension(1, 1, 100), 1);
        assert_eq!(scaled_dimension(3, 2, 4), 2);
    }
}

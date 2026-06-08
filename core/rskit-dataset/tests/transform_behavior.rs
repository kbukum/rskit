use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use rskit_dataset::transform::ResizeTransform;
use rskit_dataset::{DataItem, DatasetLimits, Label, MediaType, Transform};
use rskit_errors::ErrorCode;

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 0, 0])));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn resize_transform_validates_dimensions_and_quality() {
    assert!(matches!(
        ResizeTransform::new(0, 10),
        Err(error) if error.code() == ErrorCode::InvalidInput
    ));
    assert!(matches!(
        ResizeTransform::new(10, 0),
        Err(error) if error.code() == ErrorCode::InvalidInput
    ));

    let transform = ResizeTransform::new(2, 3).unwrap();
    assert_eq!(transform.name(), "resize-2x3");
    assert!(matches!(
        transform.clone().with_quality(0),
        Err(error) if error.code() == ErrorCode::InvalidInput
    ));
    assert!(matches!(
        transform.clone().with_quality(101),
        Err(error) if error.code() == ErrorCode::InvalidInput
    ));
    assert_eq!(transform.with_quality(100).unwrap().name(), "resize-2x3");
}

#[test]
fn resize_transform_reencodes_image_payload_and_preserves_item_metadata() {
    let limits = DatasetLimits::default();
    let item = DataItem::new(
        png_bytes(4, 2),
        Label::Real,
        MediaType::Image,
        "fixture-camera",
    )
    .unwrap()
    .with_extension(".png")
    .with_metadata("split", "train")
    .with_source_offset(42);

    let transformed = ResizeTransform::new(2, 3)
        .unwrap()
        .with_quality(90)
        .unwrap()
        .apply(item, &limits)
        .unwrap()
        .unwrap();

    assert_eq!(transformed.label, Label::Real);
    assert_eq!(transformed.media_type, MediaType::Image);
    assert_eq!(transformed.source_name, "fixture-camera");
    assert_eq!(transformed.extension, ".jpg");
    assert_eq!(transformed.metadata["split"], "train");
    assert_eq!(transformed.source_offset(), Some(42));

    let resized =
        image::load_from_memory(&transformed.payload().read_bytes_bounded(&limits).unwrap())
            .unwrap();
    assert_eq!(resized.width(), 2);
    assert_eq!(resized.height(), 3);
}

#[test]
fn resize_transform_rejects_non_image_payloads_with_typed_error() {
    let limits = DatasetLimits::default();
    let item = DataItem::new(
        b"not an image".to_vec(),
        Label::AiGenerated,
        MediaType::Text,
        "fixture-text",
    )
    .unwrap();

    let error = ResizeTransform::new(2, 2)
        .unwrap()
        .apply(item, &limits)
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert!(error.message().contains("image decode failed"));
}

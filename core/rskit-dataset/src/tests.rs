use super::*;
use rskit_errors::ErrorCode;

#[test]
fn payload_file_branches_read_write_and_same_file_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let copy = dir.path().join("copy.bin");
    let missing = dir.path().join("missing.bin");
    let blocked_destination = dir.path().join("blocked");
    std::fs::write(&source, b"payload").unwrap();
    std::fs::create_dir(&blocked_destination).unwrap();
    let limits = DatasetLimits {
        max_in_memory_bytes: 7,
        stream_buffer: 1,
    };
    let payload = DataPayload::file(&source);

    assert_eq!(payload.as_file(), Some(source.as_path()));
    assert_eq!(payload.len().unwrap(), 7);
    assert!(!payload.is_empty().unwrap());
    assert_eq!(payload.read_bytes_bounded(&limits).unwrap(), b"payload");
    assert_eq!(payload.write_to_path(&copy, &limits).unwrap(), 7);
    assert_eq!(std::fs::read(&copy).unwrap(), b"payload");
    assert_eq!(payload.write_to_path(&source, &limits).unwrap(), 7);

    let too_small = DatasetLimits {
        max_in_memory_bytes: 6,
        stream_buffer: 1,
    };
    assert_eq!(
        payload.read_bytes_bounded(&too_small).unwrap_err().code(),
        ErrorCode::InvalidInput
    );

    assert_eq!(
        DataPayload::file(&missing)
            .write_to_path(&copy, &limits)
            .unwrap_err()
            .code(),
        ErrorCode::Internal
    );
    assert_eq!(
        payload
            .write_to_path(&blocked_destination, &limits)
            .unwrap_err()
            .code(),
        ErrorCode::Internal
    );
}

#[test]
fn data_item_builders_validate_metadata_offsets_and_extensions() {
    let limits = DatasetLimits {
        max_in_memory_bytes: 3,
        stream_buffer: 1,
    };
    let item = DataItem::new(b"abc".to_vec(), Label::Real, MediaType::Text, "unit")
        .unwrap()
        .with_extension("txt")
        .with_metadata("kind", "fixture")
        .with_source_offset(42);

    assert_eq!(Label::Real.to_string(), "real");
    assert_eq!(Label::AiGenerated.to_string(), "ai");
    assert_eq!(MediaType::Image.to_string(), "image");
    assert_eq!(MediaType::Text.to_string(), "text");
    assert_eq!(MediaType::Audio.to_string(), "audio");
    assert_eq!(MediaType::Video.to_string(), "video");
    assert_eq!(item.source_offset(), Some(42));
    assert_eq!(
        item.metadata.get("kind").map(String::as_str),
        Some("fixture")
    );
    item.validate(&limits).unwrap();

    let memory_payload = DataPayload::bytes_default(vec![1, 2]).unwrap();
    assert!(memory_payload.is_bytes());
    assert_eq!(memory_payload.as_file(), None);
    assert_eq!(memory_payload.len().unwrap(), 2);
    assert!(!memory_payload.is_empty().unwrap());
    assert_eq!(
        memory_payload.read_bytes_bounded(&limits).unwrap(),
        vec![1, 2]
    );
    let output = tempfile::NamedTempFile::new().unwrap();
    assert_eq!(
        memory_payload
            .write_to_path(output.path(), &limits)
            .unwrap(),
        2
    );
    assert_eq!(std::fs::read(output.path()).unwrap(), vec![1, 2]);

    let too_small = DatasetLimits {
        max_in_memory_bytes: 1,
        stream_buffer: 1,
    };
    assert_eq!(
        DataPayload::bytes(vec![1, 2], &too_small)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        memory_payload
            .read_bytes_bounded(&too_small)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        memory_payload
            .write_to_path(output.path(), &too_small)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );

    let oversized = DataPayload::bytes(vec![1, 2, 3, 4], &DatasetLimits::default()).unwrap();
    assert_eq!(
        item.clone()
            .try_with_payload(oversized, &limits)
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        item.with_extension("../bad")
            .validate(&DatasetLimits::default())
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn data_item_validate_rejects_oversized_existing_payload_and_directory_reads() {
    let dir = tempfile::tempdir().unwrap();
    let limits = DatasetLimits {
        max_in_memory_bytes: 2,
        stream_buffer: 1,
    };
    let item = DataItem::new(b"abc".to_vec(), Label::Real, MediaType::Text, "unit").unwrap();

    assert_eq!(
        item.validate(&limits).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        DataPayload::file(dir.path())
            .read_bytes_bounded(&limits)
            .unwrap_err()
            .code(),
        ErrorCode::Internal
    );
}

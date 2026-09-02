use super::*;

#[test]
fn content_type_defaults_for_missing_or_empty_values() {
    assert_eq!(content_type_or_default(None), DEFAULT_CONTENT_TYPE);
    assert_eq!(content_type_or_default(Some("  ")), DEFAULT_CONTENT_TYPE);
    assert_eq!(content_type_or_default(Some("text/plain")), "text/plain");
    assert_eq!(content_type_or_default(Some(" text/plain ")), "text/plain");
}

#[test]
fn prefixed_key_normalizes_separator_boundaries() {
    assert_eq!(prefixed_key(None, "/file.txt"), "file.txt");
    assert_eq!(
        prefixed_key(Some("uploads/"), "/file.txt"),
        "uploads/file.txt"
    );
    assert_eq!(
        prefixed_key(Some(" /uploads/ "), "file.txt"),
        "uploads/file.txt"
    );
    assert_eq!(prefixed_key(Some("/"), "file.txt"), "file.txt");
    assert_eq!(prefixed_key(Some("///"), "/file.txt"), "file.txt");
    assert_eq!(prefixed_key(Some("uploads"), ""), "uploads/");
}

#[test]
fn stored_file_constructor_applies_defaults() {
    let stored = StoredFile::new("key", 42, None);

    assert_eq!(stored.key, "key");
    assert_eq!(stored.size, 42);
    assert_eq!(stored.content_type, DEFAULT_CONTENT_TYPE);
    assert!(stored.metadata.is_empty());
}

#[test]
fn stored_file_serde_pins_wire_field_names() {
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("owner".to_string(), "team".to_string());
    let stored = StoredFile::new("uploads/report.pdf", 1024, Some("application/pdf"))
        .with_stored_at(
            "2024-01-02T03:04:05Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        )
        .with_metadata(metadata);

    let value = serde_json::to_value(&stored).unwrap();

    assert_eq!(value["key"], "uploads/report.pdf");
    assert_eq!(value["size"], 1024);
    assert_eq!(value["content_type"], "application/pdf");
    assert_eq!(value["stored_at"], "2024-01-02T03:04:05Z");
    assert_eq!(value["metadata"]["owner"], "team");

    let round_trip: StoredFile = serde_json::from_value(value).unwrap();
    assert_eq!(round_trip.key, stored.key);
    assert_eq!(round_trip.stored_at, stored.stored_at);
    assert_eq!(round_trip.metadata, stored.metadata);
}

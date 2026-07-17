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

use super::*;
use serde_json::json;

#[test]
fn overlay_scalar_wins_last() {
    let merge = IncludeMerge::new();
    let base = json!({ "name": "base", "retries": 1 });
    let overlay = json!({ "retries": 5 });

    let merged = merge.merge(base, overlay).unwrap();

    assert_eq!(merged.get("name").unwrap().as_str(), Some("base"));
    assert_eq!(merged.get("retries").unwrap().as_i64(), Some(5));
}

#[test]
fn tables_merge_recursively() {
    let merge = IncludeMerge::new();
    let base = json!({ "server": { "host": "a", "port": 1 } });
    let overlay = json!({ "server": { "port": 2 } });

    let merged = merge.merge(base, overlay).unwrap();
    let server = merged.get("server").unwrap();

    assert_eq!(server.get("host").unwrap().as_str(), Some("a"));
    assert_eq!(server.get("port").unwrap().as_i64(), Some(2));
}

#[test]
fn identity_sections_concatenate() {
    let merge = IncludeMerge::new().with_identity("groups", IdentityKey::new("name"));
    let base = json!({ "groups": [{ "name": "a" }] });
    let overlay = json!({ "groups": [{ "name": "b" }] });

    let merged = merge.merge(base, overlay).unwrap();
    merge.validate(&merged).unwrap();
    let groups = merged.get("groups").unwrap().as_array().unwrap();

    assert_eq!(groups.len(), 2);
}

#[test]
fn duplicate_identity_across_documents_is_rejected() {
    let merge = IncludeMerge::new().with_identity("groups", IdentityKey::new("name"));
    let base = json!({ "groups": [{ "name": "dup" }] });
    let overlay = json!({ "groups": [{ "name": "dup" }] });

    let merged = merge.merge(base, overlay).unwrap();
    let err = merge.validate(&merged).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
    assert!(err.to_string().contains("dup"));
}

#[test]
fn duplicate_identity_within_single_document_is_rejected() {
    let merge = IncludeMerge::new().with_identity("groups", IdentityKey::new("name"));
    let doc = json!({ "groups": [{ "name": "x" }, { "name": "x" }] });

    let err = merge.validate(&doc).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn duplicate_identity_nested_inside_array_is_rejected() {
    // The identity section lives inside an array element,
    // so validation must recurse through arrays to catch it.
    let merge = IncludeMerge::new().with_identity("groups", IdentityKey::new("name"));
    let doc = json!({
        "tenants": [
            { "groups": [{ "name": "x" }, { "name": "x" }] }
        ]
    });

    let err = merge.validate(&doc).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn composite_identity_concatenates_distinct_edges() {
    let merge = IncludeMerge::new().with_identity("overlays", CompositeKey::new(["from", "to"]));
    let base = json!({ "overlays": [{ "from": "a", "to": "b" }] });
    let overlay = json!({ "overlays": [{ "from": "a", "to": "c" }] });

    let merged = merge.merge(base, overlay).unwrap();
    merge.validate(&merged).unwrap();

    assert_eq!(merged.get("overlays").unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn duplicate_composite_identity_is_rejected() {
    let merge = IncludeMerge::new().with_identity("overlays", CompositeKey::new(["from", "to"]));
    let base = json!({ "overlays": [{ "from": "a", "to": "b" }] });
    let overlay = json!({ "overlays": [{ "from": "a", "to": "b" }] });

    let merged = merge.merge(base, overlay).unwrap();
    let err = merge.validate(&merged).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn unique_key_sections_reject_duplicate_keys_across_documents() {
    let merge = IncludeMerge::new().with_unique_keys("groups");
    let base = json!({ "groups": { "core": { "modules": ["a"] } } });
    let overlay = json!({ "groups": { "core": { "modules": ["b"] } } });

    let err = merge.merge(base, overlay).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
    assert!(err.to_string().contains("core"));
}

#[test]
fn unique_key_sections_allow_distinct_keys() {
    let merge = IncludeMerge::new().with_unique_keys("groups");
    let base = json!({ "groups": { "core": {} } });
    let overlay = json!({ "groups": { "services": {} } });

    let merged = merge.merge(base, overlay).unwrap();
    let groups = merged.get("groups").unwrap().as_object().unwrap();

    assert!(groups.contains_key("core"));
    assert!(groups.contains_key("services"));
}

#[test]
fn non_identity_array_is_replaced() {
    let merge = IncludeMerge::new();
    let base = json!({ "ports": [1, 2, 3] });
    let overlay = json!({ "ports": [9] });

    let merged = merge.merge(base, overlay).unwrap();

    assert_eq!(merged.get("ports").unwrap().as_array().unwrap().len(), 1);
}

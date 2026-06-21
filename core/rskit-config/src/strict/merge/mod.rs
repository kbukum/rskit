//! Identity-aware include-merge for strict, layered config documents.

mod identity;

use std::collections::{BTreeMap, BTreeSet};

use rskit_codec::value::{ArrayStrategy, merge_with};
use rskit_errors::{AppError, AppResult};
use rskit_util::collections::ensure_unique_by;
use serde_json::Value;

pub use identity::{CompositeKey, IdentityKey, MergeIdentity};

/// Identity-aware include-merge configuration.
///
/// Merges config documents with deterministic, schema-aware rules:
///
/// - tables merge recursively; on a scalar key collision the overlay value wins
///   (last-wins scalars);
/// - array-of-tables sections registered via [`IncludeMerge::with_identity`] are
///   concatenated across documents and hard-error on duplicate identity;
/// - map sections registered via [`IncludeMerge::with_unique_keys`] hard-error
///   when the same map key is contributed by more than one document (a duplicate
///   identity for sections keyed by name rather than by an array element);
/// - any other array is replaced wholesale by the overlay.
#[derive(Default)]
pub struct IncludeMerge {
    identity_sections: BTreeMap<String, Box<dyn MergeIdentity>>,
    unique_key_sections: BTreeSet<String>,
}

impl std::fmt::Debug for IncludeMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncludeMerge")
            .field("identity_sections", &self.identity_sections.keys())
            .field("unique_key_sections", &self.unique_key_sections)
            .finish()
    }
}

impl IncludeMerge {
    /// Create an include-merge with no identity-keyed sections.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `section` as an identity-keyed array-of-tables.
    ///
    /// Such sections are concatenated across documents and hard-error on a
    /// duplicate identity (see [`IdentityKey`] / [`CompositeKey`]).
    #[must_use]
    pub fn with_identity(
        mut self,
        section: impl Into<String>,
        identity: impl MergeIdentity + 'static,
    ) -> Self {
        self.identity_sections
            .insert(section.into(), Box::new(identity));
        self
    }

    /// Register `section` as a map whose keys must be unique across documents.
    ///
    /// Use this for sections modelled as a table-of-tables keyed by name (for
    /// example `[groups.<name>]`): a key contributed by two documents is a
    /// duplicate identity and a hard error, rather than a silent recursive merge.
    #[must_use]
    pub fn with_unique_keys(mut self, section: impl Into<String>) -> Self {
        self.unique_key_sections.insert(section.into());
        self
    }

    /// Merge `overlay` onto `base`, returning the combined document.
    ///
    /// Delegates the value-tree mechanics to [`rskit_codec::value::merge_with`]:
    /// objects merge recursively, scalars are last-wins, and arrays under a
    /// registered identity section are concatenated (all others replaced). Before
    /// merging, any [`with_unique_keys`](Self::with_unique_keys) section is checked
    /// for keys present in both documents, since the recursive merge would
    /// otherwise silently collapse the collision.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] when a unique-key section receives the same key from
    /// both documents.
    pub fn merge(&self, base: Value, overlay: Value) -> AppResult<Value> {
        if !self.unique_key_sections.is_empty() {
            self.check_unique_keys(&base, &overlay)?;
        }
        let identity_sections = &self.identity_sections;
        Ok(merge_with(base, overlay, |key| {
            if identity_sections.contains_key(key) {
                ArrayStrategy::Concat
            } else {
                ArrayStrategy::Replace
            }
        }))
    }

    /// Validate identity-keyed sections in a (possibly merged) document.
    ///
    /// Detects duplicate identities within every registered array section,
    /// covering both single-document and merged-document cases.
    pub(crate) fn validate(&self, value: &Value) -> AppResult<()> {
        match value {
            Value::Object(table) => {
                for (key, child) in table {
                    if let (Some(identity), Value::Array(elements)) =
                        (self.identity_sections.get(key), child)
                    {
                        check_unique_identities(key, identity.as_ref(), elements)?;
                    }
                    self.validate(child)?;
                }
            }
            // Recurse into array elements so identity-keyed sections nested
            // inside a list (e.g. an object under an array) are still checked.
            Value::Array(elements) => {
                for element in elements {
                    self.validate(element)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Reject keys contributed to a unique-key section by both documents.
    ///
    /// Walks `base` and `overlay` in lockstep: for each registered section that is
    /// a table in both, an overlapping member key is a duplicate identity. Recurses
    /// through shared objects so a nested registered section is still found.
    fn check_unique_keys(&self, base: &Value, overlay: &Value) -> AppResult<()> {
        let (Value::Object(base), Value::Object(overlay)) = (base, overlay) else {
            return Ok(());
        };
        for (key, overlay_child) in overlay {
            let Some(base_child) = base.get(key) else {
                continue;
            };
            if self.unique_key_sections.contains(key)
                && let (Value::Object(base_section), Value::Object(overlay_section)) =
                    (base_child, overlay_child)
            {
                for member in overlay_section.keys() {
                    if base_section.contains_key(member) {
                        return Err(AppError::invalid_input(
                            key,
                            format!(
                                "duplicate '{member}' in section '{key}' across merged documents"
                            ),
                        ));
                    }
                }
            }
            self.check_unique_keys(base_child, overlay_child)?;
        }
        Ok(())
    }
}

fn check_unique_identities(
    section: &str,
    identity: &dyn MergeIdentity,
    elements: &[Value],
) -> AppResult<()> {
    let identities = elements
        .iter()
        .filter_map(|element| identity.identity_of(element));
    ensure_unique_by(identities, Clone::clone).map_err(|duplicate| {
        AppError::invalid_input(
            section,
            format!(
                "duplicate {} identity '{duplicate}' in section '{section}'",
                identity.label()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
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
        // The identity section lives inside an array element, so validation must
        // recurse through arrays to catch it.
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
        let merge =
            IncludeMerge::new().with_identity("overlays", CompositeKey::new(["from", "to"]));
        let base = json!({ "overlays": [{ "from": "a", "to": "b" }] });
        let overlay = json!({ "overlays": [{ "from": "a", "to": "c" }] });

        let merged = merge.merge(base, overlay).unwrap();
        merge.validate(&merged).unwrap();

        assert_eq!(merged.get("overlays").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn duplicate_composite_identity_is_rejected() {
        let merge =
            IncludeMerge::new().with_identity("overlays", CompositeKey::new(["from", "to"]));
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
}

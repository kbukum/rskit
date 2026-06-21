use std::collections::BTreeMap;

use rskit_codec::value::{ArrayStrategy, merge_with};
use rskit_errors::{AppError, AppResult};
use rskit_util::collections::find_duplicates_by;
use serde_json::Value;

/// Identity rule for an array-of-tables section during include-merge.
///
/// Sections registered with an identity are concatenated across documents rather
/// than overwritten, and duplicate identities are a hard error. Implement this
/// trait for custom identity logic, or use [`IdentityKey`] for the common case
/// of "a single named field identifies each element".
pub trait MergeIdentity: Send + Sync {
    /// Field within each array element that uniquely identifies it.
    fn identity_key(&self) -> &str;
}

/// A simple [`MergeIdentity`] naming the identity field directly.
#[derive(Debug, Clone)]
pub struct IdentityKey(String);

impl IdentityKey {
    /// Identify array elements by the value of `field`.
    pub fn new(field: impl Into<String>) -> Self {
        Self(field.into())
    }
}

impl MergeIdentity for IdentityKey {
    fn identity_key(&self) -> &str {
        &self.0
    }
}

/// Identity-aware include-merge configuration.
///
/// Merges TOML documents with deterministic, schema-aware rules:
///
/// - tables merge recursively; on a key collision the overlay value wins
///   (last-wins scalars);
/// - array-of-tables sections registered via [`IncludeMerge::with_identity`] are
///   concatenated across documents and hard-error on duplicate identity;
/// - any other array is replaced wholesale by the overlay.
#[derive(Default)]
pub struct IncludeMerge {
    identity_sections: BTreeMap<String, Box<dyn MergeIdentity>>,
}

impl std::fmt::Debug for IncludeMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncludeMerge")
            .field("identity_sections", &self.identity_sections.keys())
            .finish()
    }
}

impl IncludeMerge {
    /// Create an include-merge with no identity-keyed sections.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `section` as an identity-keyed array-of-tables.
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

    /// Merge `overlay` onto `base`, returning the combined document.
    ///
    /// Delegates the value-tree mechanics to [`rskit_codec::value::merge_with`]:
    /// objects merge recursively, scalars are last-wins, and arrays under a
    /// registered identity section are concatenated (all others replaced). This
    /// layer contributes only the policy of *which* sections are identity-keyed.
    ///
    /// # Errors
    ///
    /// Currently infallible, but returns [`AppResult`] so identity/merge policy
    /// can surface typed errors without a signature change.
    pub fn merge(&self, base: Value, overlay: Value) -> AppResult<Value> {
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
    /// Detects duplicate identities within every registered section, covering
    /// both single-document and merged-document cases.
    pub(crate) fn validate(&self, value: &Value) -> AppResult<()> {
        match value {
            Value::Object(table) => {
                for (key, child) in table {
                    if let (Some(identity), Value::Array(elements)) =
                        (self.identity_sections.get(key), child)
                    {
                        check_unique_identities(key, identity.identity_key(), elements)?;
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
}

fn check_unique_identities(section: &str, identity: &str, elements: &[Value]) -> AppResult<()> {
    // Missing or non-string identities are schema errors surfaced by the typed
    // deserialize step; the merge layer only guards duplicates.
    let identities = elements
        .iter()
        .filter_map(|element| element.get(identity).and_then(Value::as_str));
    if let Some(duplicate) = find_duplicates_by(identities, |id| *id).first() {
        return Err(AppError::invalid_input(
            section,
            format!("duplicate '{identity}' value '{duplicate}' in section '{section}'"),
        ));
    }
    Ok(())
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
    fn non_identity_array_is_replaced() {
        let merge = IncludeMerge::new();
        let base = json!({ "ports": [1, 2, 3] });
        let overlay = json!({ "ports": [9] });

        let merged = merge.merge(base, overlay).unwrap();

        assert_eq!(merged.get("ports").unwrap().as_array().unwrap().len(), 1);
    }
}

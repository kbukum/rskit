use std::path::{Component, Path, PathBuf};

use crate::golden::{Golden, GoldenMode, GoldenOutcome, Match};

const CROSS_KIT_FIXTURE_ROOT: &str = "tests/fixtures/cross-kit";

/// Golden JSON fixture rooted at `tests/fixtures/cross-kit`.
#[derive(Debug, Clone)]
pub struct CrossKitJsonGolden {
    relative: PathBuf,
}

impl CrossKitJsonGolden {
    /// Create a cross-kit JSON golden for a path below `tests/fixtures/cross-kit`.
    ///
    /// The path is validated at verification time: it must be relative and contain only normal
    /// components, so it cannot escape the fixture root.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            relative: path.as_ref().to_path_buf(),
        }
    }

    /// Verify serialized JSON against the fixture, or bless it when `RSKIT_BLESS` is set.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture path escapes the fixture root, or the underlying golden
    /// verification error when the fixture is missing or mismatched.
    pub fn verify_serialized(&self, actual: &str) -> rskit_errors::AppResult<GoldenOutcome> {
        let golden = Golden::new(cross_kit_path(&self.relative)?, Match::Exact);
        golden.run(&canonicalize_json(actual)?, GoldenMode::from_env())
    }
}

/// Resolve a fixture path under [`CROSS_KIT_FIXTURE_ROOT`], rejecting any path that could escape it.
///
/// An absolute path would replace the root when joined, and `..`/root/prefix components would climb
/// out of the tree. After that lexical check the path is confined through the canonical
/// [`rskit_fs::confine_path`], which resolves the nearest existing ancestor to also reject symlink
/// escapes before bless mode writes anything.
fn cross_kit_path(path: &Path) -> rskit_errors::AppResult<PathBuf> {
    if path.is_absolute() {
        return Err(rskit_errors::AppError::invalid_input(
            "golden json",
            format!(
                "fixture path must be relative to the fixture root: {}",
                path.display()
            ),
        ));
    }
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(rskit_errors::AppError::invalid_input(
            "golden json",
            format!(
                "fixture path must contain only normal components (no '..', '.', or root): {}",
                path.display()
            ),
        ));
    }
    // When the fixture root exists, confine through the canonical rskit-fs helper so symlinked
    // ancestors cannot redirect a write outside the tree. When it does not yet exist (first bless
    // run), there is no symlink to resolve and the lexical guarantee above is sufficient.
    let root = Path::new(CROSS_KIT_FIXTURE_ROOT);
    if root.exists() {
        rskit_fs::confine_path(root, path)
    } else {
        Ok(root.join(path))
    }
}

fn canonicalize_json(actual: &str) -> rskit_errors::AppResult<String> {
    let mut value: serde_json::Value = serde_json::from_str(actual).map_err(|err| {
        rskit_errors::AppError::invalid_input("golden json", "failed to parse JSON").with_cause(err)
    })?;
    // Sort object keys recursively so canonical output does not depend on serde_json's map
    // iteration order (which becomes insertion order if the `preserve_order` feature is unified in
    // by any dependency).
    sort_json_keys(&mut value);
    let mut json = serde_json::to_string_pretty(&value).map_err(|err| {
        rskit_errors::AppError::invalid_input("golden json", "failed to canonicalize JSON")
            .with_cause(err)
    })?;
    json.push('\n');
    Ok(json)
}

/// Recursively sort every JSON object's keys so serialization is order-independent.
fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> =
                std::mem::take(map).into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (_, child) in &mut entries {
                sort_json_keys(child);
            }
            *map = entries.into_iter().collect();
        }
        serde_json::Value::Array(items) => {
            for item in items {
                sort_json_keys(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_kit_json_golden_canonicalizes_pretty_json() {
        let outcome = CrossKitJsonGolden::new("testutil/canonical.json")
            .verify_serialized(r#"{"b":2,"a":1}"#)
            .unwrap();

        assert_eq!(outcome, GoldenOutcome::Verified);
    }

    #[test]
    fn rejects_absolute_fixture_path() {
        let err = CrossKitJsonGolden::new("/etc/passwd")
            .verify_serialized("{}")
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn rejects_parent_traversal_fixture_path() {
        let err = CrossKitJsonGolden::new("../../../etc/passwd")
            .verify_serialized("{}")
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn canonicalize_sorts_nested_keys_deterministically() {
        let a = canonicalize_json(r#"{"b":{"z":1,"a":2},"a":[{"y":1,"x":2}]}"#).unwrap();
        let b = canonicalize_json(r#"{"a":[{"x":2,"y":1}],"b":{"a":2,"z":1}}"#).unwrap();
        assert_eq!(
            a, b,
            "canonicalization must be independent of input key order"
        );
        assert!(
            a.find("\"a\"").unwrap() < a.find("\"b\"").unwrap(),
            "top-level keys must be sorted"
        );
        assert!(
            a.find("\"x\"").unwrap() < a.find("\"y\"").unwrap(),
            "nested array-object keys must be sorted"
        );
    }
}

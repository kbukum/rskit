//! Configuration normalization helpers.

use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use rskit_errors::{AppError, AppResult};

/// Return the configured schema version or the supported default.
pub fn supported_schema<T>(field: &str, configured: Option<T>, supported: T) -> AppResult<T>
where
    T: Copy + Eq + Display,
{
    let schema = configured.unwrap_or(supported);
    if schema != supported {
        return Err(AppError::invalid_input(
            field,
            format!("unsupported schema {schema}; supported schema is {supported}"),
        ));
    }
    Ok(schema)
}

/// Join a root path to a config directory when relative, then canonicalize it.
///
/// The returned path always exists because canonicalization is part of this helper.
pub fn canonicalize_root_relative_to(
    field: &str,
    config_dir: &Path,
    root: Option<&Path>,
) -> AppResult<PathBuf> {
    let root = root.unwrap_or_else(|| Path::new("."));
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        config_dir.join(root)
    };
    rskit_fs::canonicalize(&root).map_err(|error| {
        AppError::invalid_input(
            field,
            format!("failed to resolve root '{}'", root.display()),
        )
        .with_cause(error)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rskit_errors::ErrorCode;

    use super::{canonicalize_root_relative_to, supported_schema};

    #[test]
    fn supported_schema_defaults_absent_value() {
        assert_eq!(supported_schema("schema", None, 1).unwrap(), 1);
    }

    #[test]
    fn supported_schema_accepts_supported_value() {
        assert_eq!(supported_schema("schema", Some(1), 1).unwrap(), 1);
    }

    #[test]
    fn supported_schema_rejects_unsupported_value() {
        let error = supported_schema("schema", Some(2), 1).unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("unsupported schema 2"));
    }

    #[test]
    fn canonicalize_root_defaults_to_config_dir() {
        let dir = tempfile::tempdir().unwrap();

        let root = canonicalize_root_relative_to("root", dir.path(), None).unwrap();

        assert_eq!(root, std::fs::canonicalize(dir.path()).unwrap());
    }

    #[test]
    fn canonicalize_root_resolves_relative_root_against_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();

        let root = canonicalize_root_relative_to("root", dir.path(), Some(Path::new("workspace")))
            .unwrap();

        assert_eq!(root, std::fs::canonicalize(workspace).unwrap());
    }

    #[test]
    fn canonicalize_root_accepts_absolute_root() {
        let config_dir = tempfile::tempdir().unwrap();
        let root_dir = tempfile::tempdir().unwrap();

        let root = canonicalize_root_relative_to("root", config_dir.path(), Some(root_dir.path()))
            .unwrap();

        assert_eq!(root, std::fs::canonicalize(root_dir.path()).unwrap());
    }

    #[test]
    fn canonicalize_root_surfaces_canonicalization_failure() {
        let dir = tempfile::tempdir().unwrap();

        let error = canonicalize_root_relative_to("root", dir.path(), Some(Path::new("missing")))
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("failed to resolve root"));
    }
}

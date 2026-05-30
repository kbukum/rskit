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

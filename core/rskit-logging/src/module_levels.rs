//! Per-module log level overrides configured from YAML/config.
//!
//! Translates a `HashMap<String, String>` of module → level entries into a [`tracing_subscriber::EnvFilter`]
//! so that noisy dependencies can be silenced without changing the global level.
//!
//! # Example
//!
//! ```rust
//! use std::collections::HashMap;
//! use rskit_logging::module_levels::build_env_filter;
//!
//! let mut levels = HashMap::new();
//! levels.insert("sqlx".to_string(), "warn".to_string());
//! let filter = build_env_filter("info", &levels);
//! assert!(format!("{filter}").contains("sqlx"));
//! ```

use std::collections::HashMap;
use tracing_subscriber::EnvFilter;

// ── Config ──────────────────────────────────────────────────────────────────

pub use crate::config::ModuleLevelsConfig;

// ── Builder ─────────────────────────────────────────────────────────────────

/// Build an [`EnvFilter`] from a base level and per-module overrides.
///
/// The `RUST_LOG` environment variable, when set, takes precedence
/// and the returned filter reflects only `RUST_LOG`. When `RUST_LOG` is absent,
/// the filter is assembled from `base_level` plus each entry in `module_levels`.
///
/// ```text
/// base_level = "info"
/// module_levels = {"sqlx": "warn", "rdkafka": "off"}
/// → filter = "info,sqlx=warn,rdkafka=off"
/// ```
pub fn build_env_filter(base_level: &str, module_levels: &HashMap<String, String>) -> EnvFilter {
    // Respect RUST_LOG when explicitly set.
    if let Ok(filter) = EnvFilter::try_from_default_env() {
        return filter;
    }

    let directives = build_directives(base_level, module_levels);
    EnvFilter::new(&directives)
}

/// Build the raw directives string without checking `RUST_LOG`.
///
/// Useful for testing or when you want to inspect the generated filter.
pub fn build_directives(base_level: &str, module_levels: &HashMap<String, String>) -> String {
    if module_levels.is_empty() {
        return base_level.to_string();
    }

    let mut parts: Vec<String> = Vec::with_capacity(module_levels.len() + 1);
    parts.push(base_level.to_string());

    // Sort for deterministic output.
    let mut entries: Vec<_> = module_levels.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    for (module, level) in entries {
        parts.push(format!("{module}={level}"));
    }

    parts.join(",")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_module_levels_uses_base_only() {
        let directives = build_directives("info", &HashMap::new());
        assert_eq!(directives, "info");
    }

    #[test]
    fn single_override() {
        let mut levels = HashMap::new();
        levels.insert("sqlx".to_string(), "warn".to_string());
        let directives = build_directives("info", &levels);
        assert_eq!(directives, "info,sqlx=warn");
    }

    #[test]
    fn multiple_overrides_sorted() {
        let mut levels = HashMap::new();
        levels.insert("sqlx".to_string(), "warn".to_string());
        levels.insert("rdkafka".to_string(), "off".to_string());
        levels.insert("hyper".to_string(), "error".to_string());
        let directives = build_directives("debug", &levels);
        assert_eq!(directives, "debug,hyper=error,rdkafka=off,sqlx=warn");
    }

    #[test]
    fn base_level_respected() {
        let levels = HashMap::new();
        let directives = build_directives("trace", &levels);
        assert_eq!(directives, "trace");
    }

    #[test]
    fn build_env_filter_creates_valid_filter() {
        let mut levels = HashMap::new();
        levels.insert("sqlx".to_string(), "warn".to_string());
        // Should not panic.
        let _filter = build_env_filter("info", &levels);
    }

    #[test]
    fn module_levels_config_default_is_empty() {
        let cfg = ModuleLevelsConfig::default();
        assert!(cfg.levels.is_empty());
    }
}

//! Schema version constants for bench result serialization.

/// Schema version for `BenchRunResult` JSON format.
pub const SCHEMA_VERSION: &str = "1.0";

/// Schema URL for JSON validation.
pub const SCHEMA_URL: &str = "https://gokit.dev/bench/v1/schema.json";

/// Returns the schema version (for serde defaults).
pub fn version() -> String {
    SCHEMA_VERSION.to_string()
}

/// Returns the schema URL (for serde defaults).
pub fn schema_url() -> String {
    SCHEMA_URL.to_string()
}

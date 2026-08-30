//! Schema version constants for bench result serialization.
//!
//! This is a shared cross-kit contract: gokit and rskit emit byte-compatible result JSON on the
//! identity/version and metric `direction` fields. Keep [`SCHEMA_VERSION`] and [`SCHEMA_URL`] in
//! lockstep with gokit's `bench/schema.go` — a change here without the matching gokit change
//! breaks cross-language comparison of results.

/// Schema version for `BenchRunResult` JSON format. Shared with gokit's `bench.SchemaVersion`.
pub const SCHEMA_VERSION: &str = "1.0";

/// Schema URL for JSON validation. Shared cross-kit namespace with gokit's `bench.SchemaURL`.
pub const SCHEMA_URL: &str = "https://gokit.dev/bench/v1/schema.json";

/// Returns the schema version (for serde defaults).
pub fn version() -> String {
    SCHEMA_VERSION.to_string()
}

/// Returns the schema URL (for serde defaults).
pub fn schema_url() -> String {
    SCHEMA_URL.to_string()
}

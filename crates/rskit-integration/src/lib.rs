//! Integration tests for the rskit crate ecosystem.
//!
//! This crate contains end-to-end and cross-crate integration tests only.
//! There is no library code exported from this crate.
//!
//! Integration tests requiring live services (Kafka, Redis, PostgreSQL) are
//! marked `#[ignore]` and run with:
//! ```text
//! cargo nextest run --run-ignored all
//! ```

// Integration test crate — no library code, tests only.
// NOTE: integration tests live in tests/

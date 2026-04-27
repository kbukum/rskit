//! Integration test plan — see issue #33.
//!
//! These tests require live services and are ignored by default.
//! Run with: `cargo nextest run --run-ignored all`

#[tokio::test]
#[ignore = "requires live Redis — set REDIS_URL to enable"]
async fn test_cache_integration() {
    // TODO: implement cache round-trip test
}

#[tokio::test]
#[ignore = "requires live Kafka — set KAFKA_BROKERS to enable"]
async fn test_messaging_integration() {
    // TODO: implement messaging round-trip test
}

#[tokio::test]
#[ignore = "requires live PostgreSQL — set DATABASE_URL to enable"]
async fn test_database_integration() {
    // TODO: implement database round-trip test
}

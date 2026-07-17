/// Use `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` for
/// tests that exercise concurrent code paths. Plain `#[tokio::test]` uses
/// a single-threaded runtime and will NOT catch data races.
pub const CONCURRENCY_TEST_NOTE: &str =
    "Use #[tokio::test(flavor = \"multi_thread\")] for concurrency tests";

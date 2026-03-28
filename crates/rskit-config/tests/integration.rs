use rskit_config::{ConfigLoader, ServiceConfig};
use serde::Deserialize;

#[derive(Deserialize)]
struct TestConfig {
    service: ServiceConfig,
}

#[test]
fn loader_defaults_to_empty_config() {
    // No file, no env — should still produce a ConfigLoader without panicking
    let loader = ConfigLoader::new();
    // We just verify construction doesn't panic
    let _ = loader;
}

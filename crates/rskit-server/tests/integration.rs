/// Integration tests for rskit-server.
/// Full gRPC server integration tests require a live listener and are
/// marked `#[ignore]`. Run with `cargo test -- --ignored` when a
/// test gRPC server is available.

use rskit_server::config::GrpcServerConfig;

#[test]
fn default_config_is_valid() {
    let cfg = GrpcServerConfig::default();
    assert_eq!(cfg.port, 50051);
}

#[test]
#[ignore = "requires a live gRPC socket"]
fn server_starts_and_stops() {
    // TODO: spin up server, connect a test client, verify health check
}

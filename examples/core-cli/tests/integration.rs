//! Behavioural tests for `core-cli`, exercising the core-crate command logic
//! without installing a global logging subscriber.

use core_cli::commands::run;
use core_cli::settings;
use rskit_cli::CancellationToken;

fn fixture(name: &str) -> String {
    format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn loads_strict_settings_from_fixture() {
    let settings = settings::load(&fixture("app.toml")).expect("fixture should load");

    assert_eq!(settings.app_name, "core-cli-demo");
    assert_eq!(settings.workers, 3);
    assert_eq!(settings.logging.level, "debug");
}

#[test]
fn rejects_unknown_fields() {
    let err = settings::load(&fixture("unknown-key.toml")).unwrap_err();

    // deny_unknown_fields must surface the stray key as a typed parse error.
    assert!(err.to_string().contains("unexpected_key"));
}

#[test]
fn render_includes_settings() {
    let settings = settings::load(&fixture("app.toml")).expect("fixture should load");
    let rendered = core_cli::commands::show::render(&settings);

    assert!(rendered.contains("core-cli-demo"));
    assert!(rendered.contains("debug"));
}

#[tokio::test]
async fn run_processes_all_units_when_not_cancelled() {
    let token = CancellationToken::new();
    let summary = run::execute(3, &token).await.to_string();

    assert!(summary.contains("processed"));
    assert!(summary.contains('3'));
}

#[tokio::test]
async fn run_stops_immediately_when_pre_cancelled() {
    let token = CancellationToken::new();
    token.cancel();

    let summary = run::execute(5, &token).await.to_string();

    // No unit should be processed once cancellation is already requested.
    assert!(summary.contains("cancelled"));
    assert!(summary.contains("true"));
}

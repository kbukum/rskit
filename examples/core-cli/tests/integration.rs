//! Behavioural tests for `core-cli`, exercising the core-crate command logic
//! through the public library surface.

use std::path::{Path, PathBuf};

use core_cli::commands::run;
use core_cli::{cli, settings};
use rskit_cli::CancellationToken;

/// Build a portable path to a fixture under `fixtures/` using `Path::join`
/// rather than separator string formatting, so the tests pass on every OS.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

/// Extract the value rendered for `key` in an `rskit_cli::OutputKV` block,
/// which formats each pair as `  <key>:  <value>`.
fn field<'a>(rendered: &'a str, key: &str) -> Option<&'a str> {
    rendered.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

#[test]
fn loads_strict_settings_from_fixture() {
    let settings = settings::load(fixture("app.toml")).expect("fixture should load");

    assert_eq!(settings.app_name, "core-cli-demo");
    assert_eq!(settings.workers, 3);
    assert_eq!(settings.logging.level, "debug");
}

#[test]
fn rejects_unknown_fields() {
    let err = settings::load(fixture("unknown-key.toml")).unwrap_err();

    // deny_unknown_fields must surface the stray key as a typed parse error.
    assert!(err.to_string().contains("unexpected_key"));
}

#[test]
fn render_includes_settings() {
    let settings = settings::load(fixture("app.toml")).expect("fixture should load");
    let rendered = core_cli::commands::show::render(&settings);

    assert!(rendered.contains("core-cli-demo"));
    assert!(rendered.contains("debug"));
}

#[test]
fn version_render_reports_both_versions() {
    let rendered = core_cli::commands::version::render().to_string();

    assert!(rendered.contains("core-cli"));
    assert!(rendered.contains("rskit-version"));
}

#[tokio::test]
async fn run_processes_all_units_when_not_cancelled() {
    let token = CancellationToken::new();
    let summary = run::execute(3, &token).await.to_string();

    assert_eq!(field(&summary, "requested"), Some("3"));
    assert_eq!(field(&summary, "processed"), Some("3"));
    assert_eq!(field(&summary, "cancelled"), Some("false"));
}

#[tokio::test]
async fn run_stops_immediately_when_pre_cancelled() {
    let token = CancellationToken::new();
    token.cancel();

    let summary = run::execute(5, &token).await.to_string();

    // No unit should be processed once cancellation is already requested.
    assert_eq!(field(&summary, "processed"), Some("0"));
    assert_eq!(field(&summary, "cancelled"), Some("true"));
}

#[tokio::test]
async fn dispatch_version_succeeds() {
    cli::dispatch(vec!["version".to_string()])
        .await
        .expect("version should succeed");
}

#[tokio::test]
async fn dispatch_show_loads_fixture() {
    let path = fixture("app.toml").to_string_lossy().into_owned();
    cli::dispatch(vec!["show".to_string(), path])
        .await
        .expect("show should succeed");
}

#[tokio::test]
async fn dispatch_run_processes_units() {
    cli::dispatch(vec!["run".to_string(), "2".to_string()])
        .await
        .expect("run should succeed");
}

#[tokio::test]
async fn dispatch_unknown_command_errors() {
    let err = cli::dispatch(vec!["bogus".to_string()]).await.unwrap_err();
    assert!(err.to_string().contains("unknown command"));
}

#[tokio::test]
async fn dispatch_show_requires_path() {
    let err = cli::dispatch(vec!["show".to_string()]).await.unwrap_err();
    assert!(err.to_string().contains("missing"));
}

#[tokio::test]
async fn dispatch_run_rejects_non_integer() {
    let err = cli::dispatch(vec!["run".to_string(), "abc".to_string()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("integer"));
}

use parking_lot::Mutex;
use rskit_config::{AppConfig, ConfigLoader, Environment, LogFormat, ServiceConfig, load_config};
use serde::Deserialize;
use std::io::Write;
use validator::Validate;

// Serialise env-mutating tests — parallel tests share the same process env.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── Helpers ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
struct TestConfig {
    #[serde(flatten)]
    #[validate(nested)]
    service: ServiceConfig,
    #[serde(default = "default_port")]
    port: u16,
}

fn default_port() -> u16 {
    8080
}

impl rskit_config::AppConfig for TestConfig {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

#[derive(Debug, Deserialize, Validate)]
struct DefaultApplyConfig {
    #[serde(flatten)]
    #[validate(nested)]
    service: ServiceConfig,
    #[serde(default)]
    grpc_port: u16,
}

impl rskit_config::AppConfig for DefaultApplyConfig {
    fn apply_defaults(&mut self) {
        if self.grpc_port == 0 {
            self.grpc_port = 50051;
        }
    }
    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

// ── ConfigLoader builder tests ──────────────────────────────────────

#[test]
fn loader_defaults_to_empty_config() {
    let loader = ConfigLoader::new();
    let _ = loader;
}

#[test]
fn loader_new_has_debug_repr() {
    let loader = ConfigLoader::new();
    let debug = format!("{:?}", loader);
    assert!(debug.contains("ConfigLoader"));
    assert!(debug.contains("env_prefix"));
}

#[test]
fn loader_with_config_file_sets_path() {
    let loader = ConfigLoader::new().with_config_file("custom.toml");
    let debug = format!("{:?}", loader);
    assert!(debug.contains("custom.toml"));
}

#[test]
fn loader_with_env_file_sets_path() {
    let loader = ConfigLoader::new().with_env_file(".env.test");
    let debug = format!("{:?}", loader);
    assert!(debug.contains(".env.test"));
}

#[test]
fn loader_with_env_prefix_changes_prefix() {
    let loader = ConfigLoader::new().with_env_prefix("MYAPP");
    let debug = format!("{:?}", loader);
    assert!(debug.contains("MYAPP"));
}

#[test]
fn loader_builder_methods_chain() {
    let loader = ConfigLoader::new()
        .with_config_file("app.toml")
        .with_env_file(".env.local")
        .with_env_prefix("SVC");
    let debug = format!("{:?}", loader);
    assert!(debug.contains("app.toml"));
    assert!(debug.contains(".env.local"));
    assert!(debug.contains("SVC"));
}

#[test]
fn loader_default_trait_creates_valid_loader() {
    // Default derives an all-empty struct; new() sets empty prefix.
    let loader = ConfigLoader::default();
    let debug = format!("{:?}", loader);
    assert!(debug.contains("ConfigLoader"));
}

// ── ConfigLoader.load() tests ───────────────────────────────────────

#[test]
fn load_defaults_when_no_file_exists() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
    let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.service.name, "service");
    assert_eq!(cfg.service.environment, Environment::Development);
}

#[test]
fn load_env_var_overrides_default() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("PORT", "9090") };
    let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
    assert_eq!(cfg.port, 9090);
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn load_custom_prefix_env_var() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("SVC__PORT", "7777") };
    let cfg: TestConfig = ConfigLoader::new()
        .with_env_prefix("SVC")
        .load()
        .expect("should load");
    assert_eq!(cfg.port, 7777);
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("SVC__PORT") };
}

#[test]
fn load_from_toml_file() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"port = 3333\nname = \"myservice\"\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should load from TOML");
    assert_eq!(cfg.port, 3333);
    assert_eq!(cfg.service.name, "myservice");
}

#[test]
fn load_from_dotenv_file() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"PORT=4444\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_env_file(&env_path)
        .load()
        .expect("should load from .env");
    assert_eq!(cfg.port, 4444);

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn load_precedence_env_var_over_toml() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"port = 1111\n").unwrap();

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("PORT", "2222") };

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should load");
    // Env var wins over TOML
    assert_eq!(cfg.port, 2222);

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn load_precedence_dotenv_over_toml() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"port = 1111\n").unwrap();

    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"PORT=5555\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .with_env_file(&env_path)
        .load()
        .expect("should load");
    // .env sets an env var, which is read by config-rs env source → wins over TOML
    assert_eq!(cfg.port, 5555);

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn load_precedence_real_env_over_dotenv() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"PORT=5555\n").unwrap();

    // Set a real env var — should win because dotenvy does NOT overwrite existing vars
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("PORT", "6666") };

    let cfg: TestConfig = ConfigLoader::new()
        .with_env_file(&env_path)
        .load()
        .expect("should load");
    assert_eq!(cfg.port, 6666);

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

// ── load_config() convenience function ──────────────────────────────

#[test]
fn load_config_convenience_works() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
    let cfg: TestConfig = load_config().expect("convenience load should work");
    assert_eq!(cfg.port, 8080);
}

// ── AppConfig trait ─────────────────────────────────────────────────

#[test]
fn app_config_apply_defaults_is_called() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("GRPC_PORT") };

    let cfg: DefaultApplyConfig = ConfigLoader::new().load().expect("should load");
    // apply_defaults sets grpc_port to 50051 when 0
    assert_eq!(cfg.grpc_port, 50051);
}

#[test]
fn app_config_service_config_returns_reference() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let cfg: TestConfig = ConfigLoader::new().load().expect("should load");
    let svc = cfg.service_config();
    assert_eq!(svc.name, "service");
    assert_eq!(svc.environment, Environment::Development);
}

// ── Edge cases ──────────────────────────────────────────────────────

#[test]
fn invalid_toml_syntax_returns_error() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("bad.toml");
    std::fs::write(&toml_path, b"this is [[[invalid toml\n").unwrap();

    let result: Result<TestConfig, _> = ConfigLoader::new().with_config_file(&toml_path).load();
    assert!(result.is_err());
}

#[test]
fn missing_config_file_succeeds_with_defaults() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file("nonexistent_file_that_does_not_exist.toml")
        .load()
        .expect("missing file should not fail");
    assert_eq!(cfg.port, 8080);
}

#[test]
fn empty_port_env_var_fails() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("PORT", "") };

    // Empty string cannot be parsed as u16
    let result: Result<TestConfig, _> = ConfigLoader::new().load();
    assert!(result.is_err());

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn non_numeric_port_env_var_fails() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::set_var("PORT", "not_a_number") };

    let result: Result<TestConfig, _> = ConfigLoader::new().load();
    assert!(result.is_err());

    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };
}

#[test]
fn very_long_service_name_in_toml() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("PORT") };

    let long_name = "a".repeat(10_000);
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("long.toml");
    let mut f = std::fs::File::create(&toml_path).unwrap();
    write!(f, "name = \"{}\"\nport = 8080\n", long_name).unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should handle long names");
    assert_eq!(cfg.service.name.len(), 10_000);
}

#[test]
fn toml_sets_environment_production() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("ENVIRONMENT") };

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("prod.toml");
    std::fs::write(&toml_path, b"environment = \"production\"\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should load prod config");
    assert_eq!(cfg.service.environment, Environment::Production);
    assert!(cfg.service.environment.is_production());
}

#[test]
fn toml_sets_logging_config() {
    let _guard = ENV_LOCK.lock();
    // SAFETY: serialized by ENV_LOCK
    unsafe { std::env::remove_var("LOGGING__LEVEL") };

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("logging.toml");
    std::fs::write(
        &toml_path,
        b"[logging]\nlevel = \"debug\"\nformat = \"json\"\nwith_caller = true\n",
    )
    .unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should load logging config");
    assert_eq!(cfg.service.logging.level, "debug");
    assert_eq!(cfg.service.logging.format, LogFormat::Json);
    assert!(cfg.service.logging.with_caller);
}

#[test]
fn toml_sets_debug_flag() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("debug.toml");
    std::fs::write(&toml_path, b"debug = true\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load()
        .expect("should load debug config");
    assert!(cfg.service.debug);
}

#[test]
fn validation_rejects_empty_service_name_via_toml() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("empty_name.toml");
    std::fs::write(&toml_path, b"name = \"\"\nport = 8080\n").unwrap();

    let result: Result<TestConfig, _> = ConfigLoader::new().with_config_file(&toml_path).load();
    assert!(result.is_err(), "empty service name should fail validation");
}

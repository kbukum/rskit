use parking_lot::Mutex;
use rskit_config::{
    AppConfig, ConfigLoader, ConfigMapSource, DotenvFileSource, Environment, EnvironmentSource,
    LogFormat, SecretString, ServiceConfig, TomlFileSource, load_config,
};
use rskit_validation::Validate;
use serde::Deserialize;
use std::io::Write;

// Serialise env-mutating tests — parallel tests share the same process env.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── Helpers ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TestConfig {
    #[serde(flatten)]
    service: ServiceConfig,
    #[serde(default = "default_app_port")]
    app_port: u16,
}

fn default_app_port() -> u16 {
    8080
}

impl rskit_config::AppConfig for TestConfig {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

impl Validate for TestConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        self.service.validate()
    }
}

#[derive(Debug, Deserialize)]
struct DefaultApplyConfig {
    #[serde(flatten)]
    service: ServiceConfig,
    #[serde(default)]
    grpc_port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolConfig {
    name: String,
    #[serde(default)]
    retries: u16,
}

#[derive(Debug, Deserialize)]
struct SecretConfig {
    api_token: SecretString,
}

#[derive(Debug, Deserialize)]
struct NameOnlyConfig {
    name: String,
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

impl Validate for DefaultApplyConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        self.service.validate()
    }
}

impl Validate for ToolConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        let mut errors = validator::ValidationErrors::new();
        if self.name.trim().is_empty() {
            errors.add("name", validator::ValidationError::new("length"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for SecretConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

impl Validate for NameOnlyConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

fn set_required_env() {
    set_env("ADDRESS", "127.0.0.1");
    set_env("PORT", "50051");
}

fn clear_required_env() {
    remove_env("ADDRESS");
    remove_env("PORT");
}

fn set_env(key: &str, value: impl AsRef<std::ffi::OsStr>) {
    // SAFETY: all tests that mutate process environment hold ENV_LOCK, which
    // serializes access to Rust 2024's process-global environment state.
    unsafe { std::env::set_var(key, value) };
}

fn remove_env(key: &str) {
    // SAFETY: all tests that mutate process environment hold ENV_LOCK, which
    // serializes access to Rust 2024's process-global environment state.
    unsafe { std::env::remove_var(key) };
}

// ── Strict typed TOML tests ──────────────────────────────────────────

#[test]
fn toml_loader_loads_non_service_config() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("tool.toml");
    std::fs::write(&toml_path, b"name = \"toven\"\nretries = 2\n").unwrap();

    let cfg: ToolConfig = ConfigLoader::toml(&toml_path)
        .load()
        .expect("should load tool config");

    assert_eq!(cfg.name, "toven");
    assert_eq!(cfg.retries, 2);
}

#[test]
fn toml_loader_rejects_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("tool.toml");
    std::fs::write(&toml_path, b"name = \"toven\"\nextra = true\n").unwrap();

    let err = ConfigLoader::toml(&toml_path)
        .load::<ToolConfig>()
        .expect_err("unknown key should fail");

    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn toml_loader_ignores_environment_variables() {
    let _guard = ENV_LOCK.lock();
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("tool.toml");
    std::fs::write(&toml_path, b"name = \"toven\"\nretries = 2\n").unwrap();

    set_env("RETRIES", "9");
    let cfg: ToolConfig = ConfigLoader::toml(&toml_path)
        .load()
        .expect("should load tool config");
    remove_env("RETRIES");

    assert_eq!(cfg.retries, 2);
}

#[test]
fn toml_loader_load_with_applies_defaults_before_validation() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("tool.toml");
    std::fs::write(&toml_path, b"name = \"toven\"\n").unwrap();

    let cfg: ToolConfig = ConfigLoader::toml(&toml_path)
        .load_with(|cfg: &mut ToolConfig| cfg.retries = 3)
        .expect("should apply defaults");

    assert_eq!(cfg.retries, 3);
}

#[test]
fn toml_loader_deserializes_secret_string_with_redacted_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("secret.toml");
    std::fs::write(&toml_path, b"api_token = \"super-secret-token\"\n").unwrap();

    let cfg: SecretConfig = ConfigLoader::toml(&toml_path)
        .load()
        .expect("should load secret config");

    assert_eq!(cfg.api_token.expose(), "super-secret-token");
    assert_eq!(cfg.api_token.to_string(), "***");

    let debug = format!("{cfg:?}");
    assert!(debug.contains("SecretString(***)"));
    assert!(!debug.contains("super-secret-token"));
}

#[test]
fn dotenv_source_deserializes_secret_string_without_mutating_environment() {
    let _guard = ENV_LOCK.lock();
    remove_env("API_TOKEN");

    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"API_TOKEN=dotenv-secret\n").unwrap();

    let cfg: SecretConfig = ConfigLoader::custom()
        .with_source(DotenvFileSource::required(&env_path, ""))
        .load()
        .expect("should load secret from dotenv source");

    assert_eq!(cfg.api_token.expose(), "dotenv-secret");
    assert!(std::env::var("API_TOKEN").is_err());
}

#[test]
fn required_dotenv_source_rejects_malformed_files() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"not valid dotenv\n").unwrap();

    let err = ConfigLoader::custom()
        .with_source(DotenvFileSource::required(&env_path, ""))
        .load::<NameOnlyConfig>()
        .expect_err("required dotenv parsing should fail closed");

    assert!(err.to_string().contains("failed to parse env file"));
}

#[test]
fn environment_source_without_prefix_reads_unprefixed_values() {
    let _guard = ENV_LOCK.lock();
    let previous = std::env::var("NAME").ok();
    set_env("NAME", "from-env");

    let cfg: NameOnlyConfig = ConfigLoader::custom()
        .with_source(EnvironmentSource::new())
        .load()
        .expect("should load unprefixed environment value");

    assert_eq!(cfg.name, "from-env");
    if let Some(value) = previous {
        set_env("NAME", value);
    } else {
        remove_env("NAME");
    }
}

#[test]
fn toml_file_source_exposes_configured_path() {
    let source = TomlFileSource::required("config/settings.toml");

    assert_eq!(source.path(), std::path::Path::new("config/settings.toml"));
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
        .with_default("app_port", 7000_i64)
        .with_config_file("app.toml")
        .with_env_file(".env.local")
        .with_env_prefix("SVC")
        .with_override("app_port", 9000_i64);
    let debug = format!("{:?}", loader);
    assert!(debug.contains("app.toml"));
    assert!(debug.contains(".env.local"));
    assert!(debug.contains("SVC"));
    assert!(debug.contains("app_port"));
}

#[test]
fn loader_default_trait_creates_valid_loader() {
    // Default derives an all-empty struct; new() sets empty prefix.
    let loader = ConfigLoader::default();
    let debug = format!("{:?}", loader);
    assert!(debug.contains("ConfigLoader"));
}

// ── ConfigLoader.load_app() tests ───────────────────────────────────

#[test]
fn load_defaults_when_no_file_exists() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    remove_env("APP_PORT");
    let cfg: TestConfig = ConfigLoader::new().load_app().expect("should load");
    assert_eq!(cfg.app_port, 8080);
    assert_eq!(cfg.service.name, "service");
    assert_eq!(cfg.service.environment, Environment::Development);
    clear_required_env();
}

#[test]
fn load_env_var_overrides_default() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    set_env("APP_PORT", "9090");
    let cfg: TestConfig = ConfigLoader::new().load_app().expect("should load");
    assert_eq!(cfg.app_port, 9090);
    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn load_custom_prefix_env_var() {
    let _guard = ENV_LOCK.lock();
    set_env("SVC__ADDRESS", "127.0.0.1");
    set_env("SVC__PORT", "50051");
    set_env("SVC__APP_PORT", "7777");
    let cfg: TestConfig = ConfigLoader::new()
        .with_env_prefix("SVC")
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 7777);
    remove_env("SVC__ADDRESS");
    remove_env("SVC__PORT");
    remove_env("SVC__APP_PORT");
}

#[test]
fn load_from_toml_file() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"app_port = 3333\nname = \"myservice\"\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load from TOML");
    assert_eq!(cfg.app_port, 3333);
    assert_eq!(cfg.service.name, "myservice");
    clear_required_env();
}

#[test]
fn load_from_dotenv_file() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"APP_PORT=4444\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_env_file(&env_path)
        .load_app()
        .expect("should load from .env");
    assert_eq!(cfg.app_port, 4444);

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn load_precedence_env_var_over_toml() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"app_port = 1111\n").unwrap();

    set_env("APP_PORT", "2222");

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 2222);

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn load_precedence_dotenv_over_toml() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"app_port = 1111\n").unwrap();

    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"APP_PORT=5555\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .with_env_file(&env_path)
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 5555);

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn load_precedence_real_env_over_dotenv() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, b"APP_PORT=5555\n").unwrap();

    set_env("APP_PORT", "6666");

    let cfg: TestConfig = ConfigLoader::new()
        .with_env_file(&env_path)
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 6666);

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn load_precedence_file_over_programmatic_default() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"app_port = 2222\n").unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_default("app_port", 1111_i64)
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 2222);

    clear_required_env();
}

#[test]
fn load_precedence_programmatic_override_wins() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(&toml_path, b"app_port = 1111\n").unwrap();
    set_env("APP_PORT", "2222");

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .with_override("app_port", 3333_i64)
        .load_app()
        .expect("should load");
    assert_eq!(cfg.app_port, 3333);

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn app_adapter_source_loads_between_dotenv_and_environment() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    set_env("APP_PORT", "4444");

    let source = ConfigMapSource::new().with_value("app_port", 3333_i64);
    let cfg: TestConfig = ConfigLoader::new()
        .with_source(source)
        .load_app()
        .expect("should load with adapter source");

    assert_eq!(cfg.app_port, 4444);
    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn custom_loader_uses_only_explicit_adapter_sources() {
    let source = ConfigMapSource::new()
        .with_value("name", "toven")
        .with_value("retries", 4_i64);

    let cfg: ToolConfig = ConfigLoader::custom()
        .with_source(source)
        .load()
        .expect("should load from explicit source");

    assert_eq!(cfg.name, "toven");
    assert_eq!(cfg.retries, 4);
}

// ── load_config() convenience function ──────────────────────────────

#[test]
fn load_config_convenience_works() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    let cfg: TestConfig = load_config().expect("convenience load should work");
    assert_eq!(cfg.app_port, 8080);
    clear_required_env();
}

// ── AppConfig trait ─────────────────────────────────────────────────

#[test]
fn app_config_apply_defaults_is_called() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    remove_env("GRPC_PORT");

    let cfg: DefaultApplyConfig = ConfigLoader::new().load_app().expect("should load");
    assert_eq!(cfg.grpc_port, 50051);
    clear_required_env();
}

#[test]
fn app_config_service_config_returns_reference() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let cfg: TestConfig = ConfigLoader::new().load_app().expect("should load");
    let svc = cfg.service_config();
    assert_eq!(svc.name, "service");
    assert_eq!(svc.environment, Environment::Development);
    clear_required_env();
}

// ── Edge cases ──────────────────────────────────────────────────────

#[test]
fn invalid_toml_syntax_returns_error() {
    let _guard = ENV_LOCK.lock();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("bad.toml");
    std::fs::write(&toml_path, b"this is [[[invalid toml\n").unwrap();

    let result: Result<TestConfig, _> = ConfigLoader::new().with_config_file(&toml_path).load_app();
    assert!(result.is_err());
}

#[test]
fn missing_config_file_succeeds_with_defaults() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file("nonexistent_file_that_does_not_exist.toml")
        .load_app()
        .expect("missing file should not fail");
    assert_eq!(cfg.app_port, 8080);
    clear_required_env();
}

#[test]
fn profile_from_environment_requires_environment_value() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    let previous = std::env::var("ENVIRONMENT").ok();
    remove_env("ENVIRONMENT");

    let result: Result<TestConfig, _> = ConfigLoader::new().with_profile("").load_app();
    assert!(result.is_err());

    if let Some(value) = previous {
        set_env("ENVIRONMENT", value);
    }
    clear_required_env();
}

#[test]
fn explicit_profile_requires_profile_env_file() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let result: Result<TestConfig, _> = ConfigLoader::new()
        .with_profile("profile_that_should_not_exist_for_tests")
        .load_app();
    assert!(result.is_err());

    clear_required_env();
}

#[test]
fn empty_port_env_var_fails() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    set_env("APP_PORT", "");

    let result: Result<TestConfig, _> = ConfigLoader::new().load_app();
    assert!(result.is_err());

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn non_numeric_port_env_var_fails() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    set_env("APP_PORT", "not_a_number");

    let result: Result<TestConfig, _> = ConfigLoader::new().load_app();
    assert!(result.is_err());

    remove_env("APP_PORT");
    clear_required_env();
}

#[test]
fn very_long_service_name_in_toml() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let long_name = "a".repeat(10_000);
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("long.toml");
    let mut f = std::fs::File::create(&toml_path).unwrap();
    write!(
        f,
        "name = \"{}\"\naddress = \"0.0.0.0\"\nport = 50051\n",
        long_name
    )
    .unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should handle long names");
    assert_eq!(cfg.service.name.len(), 10_000);
    clear_required_env();
}

#[test]
fn toml_sets_environment_production() {
    let _guard = ENV_LOCK.lock();
    set_required_env();
    remove_env("ENVIRONMENT");

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("prod.toml");
    std::fs::write(
        &toml_path,
        b"environment = \"production\"\naddress=\"0.0.0.0\"\nport=50051\n",
    )
    .unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load prod config");
    assert_eq!(cfg.service.environment, Environment::Production);
    assert!(cfg.service.environment.is_production());
    clear_required_env();
}

#[test]
fn toml_sets_logging_config() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("logging.toml");
    std::fs::write(
        &toml_path,
        b"address=\"0.0.0.0\"\nport=50051\n[logging]\nlevel = \"debug\"\nformat = \"json\"\nwith_caller = true\n",
    )
    .unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load logging config");
    assert_eq!(cfg.service.logging.level, "debug");
    assert_eq!(cfg.service.logging.format, LogFormat::Json);
    assert!(cfg.service.logging.with_caller);
    clear_required_env();
}

#[test]
fn toml_sets_debug_flag() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("debug.toml");
    std::fs::write(
        &toml_path,
        b"debug = true\naddress=\"0.0.0.0\"\nport=50051\n",
    )
    .unwrap();

    let cfg: TestConfig = ConfigLoader::new()
        .with_config_file(&toml_path)
        .load_app()
        .expect("should load debug config");
    assert!(cfg.service.debug);
    clear_required_env();
}

#[test]
fn validation_rejects_empty_service_name_via_toml() {
    let _guard = ENV_LOCK.lock();
    set_required_env();

    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("empty_name.toml");
    std::fs::write(
        &toml_path,
        b"name = \"\"\naddress=\"0.0.0.0\"\nport=50051\n",
    )
    .unwrap();

    let result: Result<TestConfig, _> = ConfigLoader::new().with_config_file(&toml_path).load_app();
    assert!(result.is_err(), "empty service name should fail validation");
    clear_required_env();
}

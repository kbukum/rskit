use std::path::{Path, PathBuf};

use rskit_codec::{Codec, TomlCodec};
use rskit_errors::{AppError, AppResult};
use rskit_fs::sync_io::file;
use serde_json::{Map as JsonMap, Number, Value as JsonValue};

use super::contract::ConfigSource;

/// Upper bound on a single configuration file accepted on read (1 MiB).
///
/// Config files are small by design; a bound keeps decoding cheap and, for YAML,
/// caps alias-expansion blow-up, which the codec requires to be fed bounded input.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// TOML file source.
#[derive(Debug, Clone)]
pub struct TomlFileSource {
    path: PathBuf,
    required: bool,
}

impl TomlFileSource {
    /// Create a required TOML file source.
    pub fn required(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: true,
        }
    }

    /// Create an optional TOML file source.
    pub fn optional(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: false,
        }
    }

    /// Return the configured path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigSource for TomlFileSource {
    fn collect(&self) -> AppResult<config::Config> {
        let Some(contents) = read_optional_config_file(&self.path, self.required, "TOML")? else {
            return config_from_json_object(JsonMap::new());
        };
        let value = TomlCodec.decode_value(&contents).map_err(|err| {
            AppError::invalid_input(
                "config",
                format!(
                    "failed to load TOML config '{}': {err}",
                    self.path.display()
                ),
            )
            .with_cause(err)
        })?;
        json_value_to_config(value)
    }
}

pub(super) fn read_optional_config_file(
    path: &Path,
    required: bool,
    format: &str,
) -> AppResult<Option<String>> {
    if !required && !file::exists(path)? {
        return Ok(None);
    }
    file::read_string_bounded(path, MAX_CONFIG_BYTES)
        .map(Some)
        .map_err(|err| {
            AppError::invalid_input(
                "config",
                format!("failed to read {format} config '{}': {err}", path.display()),
            )
            .with_cause(err)
        })
}

pub(super) fn json_value_to_config(value: JsonValue) -> AppResult<config::Config> {
    match value {
        JsonValue::Object(object) => config_from_json_object(object),
        _ => Err(AppError::invalid_input(
            "config",
            "config top level must be an object",
        )),
    }
}

fn config_from_json_object(object: JsonMap<String, JsonValue>) -> AppResult<config::Config> {
    let mut builder = config::Config::builder();
    for (key, value) in object {
        builder = builder
            .set_default(&key, json_to_config_value(value))
            .map_err(|err| AppError::invalid_input("config", err.to_string()))?;
    }
    builder
        .build()
        .map_err(|err| AppError::invalid_input("config", err.to_string()))
}

fn json_to_config_value(value: JsonValue) -> config::Value {
    config::Value::new(None, json_to_config_kind(value))
}

fn json_to_config_kind(value: JsonValue) -> config::ValueKind {
    match value {
        JsonValue::Null => config::ValueKind::Nil,
        JsonValue::Bool(value) => config::ValueKind::Boolean(value),
        JsonValue::Number(value) => number_to_config_kind(value),
        JsonValue::String(value) => config::ValueKind::String(value),
        JsonValue::Array(values) => {
            config::ValueKind::Array(values.into_iter().map(json_to_config_value).collect())
        }
        JsonValue::Object(values) => config::ValueKind::Table(
            values
                .into_iter()
                .map(|(key, value)| (key, json_to_config_value(value)))
                .collect(),
        ),
    }
}

fn number_to_config_kind(value: Number) -> config::ValueKind {
    if let Some(value) = value.as_i64() {
        return config::ValueKind::I64(value);
    }
    if let Some(value) = value.as_u64() {
        return config::ValueKind::U64(value);
    }
    config::ValueKind::Float(value.as_f64().unwrap_or_default())
}

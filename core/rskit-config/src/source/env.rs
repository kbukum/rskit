use rskit_errors::{AppError, AppResult};

use super::contract::ConfigSource;

/// Environment-variable configuration source.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentSource {
    prefix: String,
}

impl EnvironmentSource {
    /// Create an environment source without a prefix.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an environment source with a prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl ConfigSource for EnvironmentSource {
    fn collect(&self) -> AppResult<config::Config> {
        let mut builder = config::Config::builder();
        for (key, value) in std::env::vars_os() {
            // Environment data is an external boundary: `vars()` panics on non-Unicode
            // entries, so read the OS-string form and skip anything that is not valid UTF-8.
            let (Some(key), Some(value)) = (key.to_str(), value.to_str()) else {
                continue;
            };
            if let Some(key) = normalize_env_key(&self.prefix, key) {
                builder = builder
                    .set_override(&key, parse_env_value(value.to_owned()))
                    .map_err(|e| AppError::invalid_input("config", e.to_string()))?;
            }
        }
        builder
            .build()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))
    }
}

pub(crate) fn normalize_env_key(prefix: &str, key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    if prefix.is_empty() {
        return normalized_config_key(&key);
    }

    let prefix = prefix.to_ascii_lowercase();
    key.strip_prefix(&format!("{prefix}__"))
        .and_then(normalized_config_key)
}

fn normalized_config_key(key: &str) -> Option<String> {
    let key = key.replace("__", ".");
    let valid = key.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    });
    valid.then_some(key)
}

pub(crate) fn parse_env_value(value: String) -> config::Value {
    if let Ok(value) = value.parse::<bool>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<i64>() {
        return value.into();
    }
    if let Ok(value) = value.parse::<f64>() {
        return value.into();
    }
    value.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_env_key_handles_prefixes_and_nested_keys() {
        assert_eq!(
            normalize_env_key("", "SERVICE__PORT").as_deref(),
            Some("service.port")
        );
        assert_eq!(
            normalize_env_key("APP", "APP__SERVICE__DEBUG").as_deref(),
            Some("service.debug")
        );
        assert_eq!(normalize_env_key("APP", "OTHER__VALUE"), None);
    }

    #[test]
    fn parse_env_value_preserves_basic_types() {
        assert!(parse_env_value("true".to_string()).into_bool().unwrap());
        assert_eq!(parse_env_value("42".to_string()).into_int().unwrap(), 42);
        assert_eq!(
            parse_env_value("1.5".to_string()).into_float().unwrap(),
            1.5
        );
        assert_eq!(
            parse_env_value("plain".to_string()).into_string().unwrap(),
            "plain"
        );
    }
}

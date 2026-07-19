//! Configuration for the Anthropic provider.

use rskit_util::SecretString;
use serde::Deserialize;

/// Anthropic provider configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Anthropic API key.
    ///
    /// The value is redacted in debug/display output,
    /// and adapters pass it to `rskit-httpclient` as redacting auth state.
    pub api_key: SecretString,

    /// Base URL (default: `https://api.anthropic.com`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Chat model name (e.g. `claude-sonnet-4-20250514`).
    #[serde(default = "default_model")]
    pub model: String,

    /// Anthropic API version header value.
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

fn default_base_url() -> String {
    "https://api.anthropic.com".into()
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".into()
}

fn default_api_version() -> String {
    "2023-06-01".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize_with_defaults() {
        let json = r#"{"api_key":"sk-ant-test"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key.expose(), "sk-ant-test");
        assert_eq!(cfg.base_url, "https://api.anthropic.com");
        assert_eq!(cfg.model, "claude-sonnet-4-20250514");
        assert_eq!(cfg.api_version, "2023-06-01");
    }

    #[test]
    fn config_custom_values() {
        let json = r#"{
            "api_key":"key",
            "base_url":"http://proxy",
            "model":"claude-3-haiku",
            "api_version":"2024-01-01"
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://proxy");
        assert_eq!(cfg.model, "claude-3-haiku");
        assert_eq!(cfg.api_version, "2024-01-01");
    }
}

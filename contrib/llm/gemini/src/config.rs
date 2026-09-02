//! Configuration for the Gemini provider.

use rskit_llm_common::HttpTransportConfig;
use rskit_util::SecretString;
use serde::Deserialize;

/// Gemini provider configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Google AI API key.
    ///
    /// The value is redacted in debug/display output,
    /// and adapters pass it to `rskit-httpclient` as redacting auth state.
    pub api_key: SecretString,

    /// Base URL (default: `https://generativelanguage.googleapis.com`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Model name (e.g. `gemini-2.5-flash`).
    #[serde(default = "default_model")]
    pub model: String,

    /// Optional HTTP transport tuning (timeout, headers, TLS, resilience).
    #[serde(default, flatten)]
    pub transport: HttpTransportConfig,
}

fn default_base_url() -> String {
    "https://generativelanguage.googleapis.com".into()
}

fn default_model() -> String {
    "gemini-2.5-flash".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize_with_defaults() {
        let json = r#"{"api_key":"AIza-test"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.api_key.expose(), "AIza-test");
        assert_eq!(cfg.base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(cfg.model, "gemini-2.5-flash");
    }

    #[test]
    fn config_custom_values() {
        let json = r#"{
            "api_key":"key",
            "base_url":"http://localhost:9000",
            "model":"gemini-2.0-flash"
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://localhost:9000");
        assert_eq!(cfg.model, "gemini-2.0-flash");
    }
}

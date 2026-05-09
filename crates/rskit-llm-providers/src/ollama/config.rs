//! Configuration for the Ollama provider.

use serde::Deserialize;

/// Ollama provider configuration.
///
/// Ollama exposes an OpenAI-compatible chat-completions endpoint at
/// `<base_url>/v1/chat/completions`. No API key is required for a local
/// instance, but one may be set for remote/proxied deployments.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Base URL of the Ollama server (default: `http://localhost:11434`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Chat model name (e.g. `llama3.2`, `mistral`, `phi3`).
    #[serde(default = "default_model")]
    pub model: String,

    /// Optional API key for remote/proxied Ollama instances.
    #[serde(default)]
    pub api_key: Option<String>,
}

fn default_base_url() -> String {
    "http://localhost:11434".into()
}

fn default_model() -> String {
    "llama3.2".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.base_url, "http://localhost:11434");
        assert_eq!(cfg.model, "llama3.2");
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn config_custom_values() {
        let json = r#"{"base_url":"http://192.168.1.10:11434","model":"mistral","api_key":"tok"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://192.168.1.10:11434");
        assert_eq!(cfg.model, "mistral");
        assert_eq!(cfg.api_key.as_deref(), Some("tok"));
    }
}

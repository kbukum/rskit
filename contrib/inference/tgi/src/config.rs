use rskit_util::SecretString;
use serde::{Deserialize, Serialize};

use crate::TGI_KIND;

/// Configuration for the TGI OAI-compatible adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Base URL of the TGI server, for example `http://localhost:8080`.
    pub base_url: String,
    /// Default model identifier.
    #[serde(default = "default_model")]
    pub model: String,
    /// Optional bearer token for authenticated TGI deployments.
    ///
    /// The value is redacted in debug output and serialization.
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// Max new tokens for generation.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    TGI_KIND.into()
}

fn default_max_tokens() -> u32 {
    256
}

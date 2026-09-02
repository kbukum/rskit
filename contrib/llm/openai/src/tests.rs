use super::*;
use rskit_llm_common::HttpTransportConfig;

fn config() -> Config {
    Config {
        api_key: rskit_util::SecretString::new("sk-test"),
        base_url: "https://api.openai.com/v1".into(),
        model: "gpt-4o".into(),
        embedding_model: "text-embedding-3-small".into(),
        embedding_dimensions: Some(1536),
        transport: HttpTransportConfig::default(),
    }
}

#[test]
fn embedding_provider_builders_construct_providers() {
    let cfg = config();
    embedding_provider(&cfg).unwrap();
    embedding_provider_with_policy(&cfg, rskit_resilience::Policy::default()).unwrap();
}

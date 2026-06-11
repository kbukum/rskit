#![allow(missing_docs)]

use rskit_ai::{Capabilities, Model, Provider as ModelProvider};
use rskit_embedding::{
    EmbedAsset, EmbedInput, EmbedRequest, Embedding, EmbeddingOptions, InMemoryProvider, Provider,
    Usage,
};
use rskit_provider::{Provider as _, RequestResponse};
use serde_json::json;

fn model() -> Model {
    Model {
        name: "embed-model".into(),
        provider: ModelProvider::Custom("memory".into()),
        version: Some("v1".into()),
        capabilities: Capabilities::default(),
    }
}

#[test]
fn embedding_options_require_json_objects_and_round_trip() {
    let options = EmbeddingOptions::new(json!({"dimensions": 3})).unwrap();
    assert_eq!(options.as_json()["dimensions"], 3);
    assert_eq!(options.clone().into_json(), json!({"dimensions": 3}));
    assert!(EmbeddingOptions::new(json!(["not", "object"])).is_err());
    assert!(serde_json::from_value::<EmbeddingOptions>(json!(false)).is_err());
}

#[test]
fn embedding_constructor_records_dimensions_and_index() {
    let embedding = Embedding::new(vec![1.0, 2.0, 3.0], 9);
    assert_eq!(embedding.dimensions, 3);
    assert_eq!(embedding.index, 9);
}

#[tokio::test]
async fn in_memory_provider_is_deterministic_for_text_and_assets() {
    let provider = InMemoryProvider::new(4);
    let request = EmbedRequest {
        model: model(),
        inputs: vec![
            EmbedInput::Text("hello".into()),
            EmbedInput::Image(EmbedAsset::Bytes(vec![1, 2, 3])),
            EmbedInput::Audio(EmbedAsset::Url("https://example.com/a.wav".into())),
            EmbedInput::Video(EmbedAsset::Bytes(vec![9, 8, 7])),
        ],
        options: EmbeddingOptions::default(),
    };

    let first = provider.embed(request.clone()).await.unwrap();
    let second = provider.execute(request).await.unwrap();
    assert_eq!(first.embeddings, second.embeddings);
    assert_eq!(first.embeddings.len(), 4);
    assert!(
        first
            .embeddings
            .iter()
            .all(|embedding| embedding.dimensions == 4)
    );
    assert_eq!(first.usage, Usage::default());
    assert_eq!(provider.name(), "in_memory_embedding");
}

#[tokio::test]
async fn batch_and_component_contracts_are_noops_but_report_health() {
    let provider = InMemoryProvider::default();
    rskit_component::Component::start(&provider).await.unwrap();
    let health = rskit_component::Component::health(&provider);
    assert!(health.is_healthy());
    assert_eq!(
        rskit_component::Component::name(&provider),
        "rskit-embedding.in_memory"
    );
    rskit_component::Component::stop(&provider).await.unwrap();

    let request = EmbedRequest {
        model: model(),
        inputs: vec![EmbedInput::Text("batch".into())],
        options: EmbeddingOptions::default(),
    };
    let responses = provider
        .embed_batch(vec![request.clone(), request])
        .await
        .unwrap();
    assert_eq!(responses.len(), 2);
}

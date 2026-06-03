//! Comprehensive contract tests for rskit-provider.

use futures::{StreamExt, stream};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_provider::{
    Binding, Duplex, DuplexChannel, Provider, Registry, RequestResponse, Sink, Stream,
    TowerProvider, request_response_fn, sink_fn, traits::BoxStream,
};

struct EchoProvider;

#[async_trait::async_trait]
impl Provider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }
}

#[async_trait::async_trait]
impl RequestResponse<String, String> for EchoProvider {
    async fn execute(&self, input: String) -> AppResult<String> {
        Ok(input.to_uppercase())
    }
}

struct VecStream;

#[async_trait::async_trait]
impl Provider for VecStream {
    fn name(&self) -> &'static str {
        "vec-stream"
    }
}

#[async_trait::async_trait]
impl Stream<(), i32> for VecStream {
    async fn execute(&self, _input: ()) -> AppResult<BoxStream<i32>> {
        Ok(Box::pin(stream::iter([Ok(1), Ok(2), Ok(3)])))
    }
}

struct CollectSink;

#[async_trait::async_trait]
impl Provider for CollectSink {
    fn name(&self) -> &'static str {
        "collect"
    }
}

#[async_trait::async_trait]
impl Sink<String> for CollectSink {
    async fn send(&self, input: String) -> AppResult<()> {
        if input.is_empty() {
            Err(AppError::new(ErrorCode::InvalidInput, "empty input"))
        } else {
            Ok(())
        }
    }
}

struct MemoryChannel {
    values: Vec<String>,
}

#[async_trait::async_trait]
impl DuplexChannel<String, String> for MemoryChannel {
    async fn send(&mut self, input: String) -> AppResult<()> {
        self.values.push(input);
        Ok(())
    }

    async fn recv(&mut self) -> AppResult<Option<String>> {
        Ok(self.values.pop())
    }

    async fn close(&mut self) -> AppResult<()> {
        self.values.clear();
        Ok(())
    }
}

struct ChatDuplex;

#[async_trait::async_trait]
impl Provider for ChatDuplex {
    fn name(&self) -> &'static str {
        "chat"
    }
}

#[async_trait::async_trait]
impl Duplex<String, String> for ChatDuplex {
    async fn open(&self) -> AppResult<Box<dyn DuplexChannel<String, String>>> {
        Ok(Box::new(MemoryChannel { values: Vec::new() }))
    }
}

#[tokio::test]
async fn request_response_executes() {
    let provider = EchoProvider;
    assert_eq!(
        provider.execute("hello".to_string()).await.unwrap(),
        "HELLO"
    );
    assert_eq!(provider.name(), "echo");
}

#[tokio::test]
async fn stream_executes_to_box_stream() {
    let provider = VecStream;
    let values = provider
        .execute(())
        .await
        .unwrap()
        .map(|item| item.unwrap())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(values, vec![1, 2, 3]);
}

#[tokio::test]
async fn sink_sends_or_returns_typed_error() {
    let sink = CollectSink;
    assert!(sink.send("value".to_string()).await.is_ok());
    let error = sink.send(String::new()).await.unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn duplex_channel_round_trips() {
    let duplex = ChatDuplex;
    let mut channel = duplex.open().await.unwrap();
    channel.send("hello".to_string()).await.unwrap();
    assert_eq!(channel.recv().await.unwrap(), Some("hello".to_string()));
    channel.close().await.unwrap();
}

#[tokio::test]
async fn closure_adapters_expose_canonical_traits() {
    let rr = request_response_fn("double", |input: i32| async move { Ok(input * 2) });
    assert_eq!(rr.execute(21).await.unwrap(), 42);

    let sink = sink_fn("sink", |_input: i32| async move { Ok(()) });
    assert!(sink.send(1).await.is_ok());
}

#[tokio::test]
async fn tower_provider_bridges_request_response() {
    let service = tower::service_fn(|input: i32| async move { Ok::<_, AppError>(input + 1) });
    let provider = TowerProvider::new("tower", service);
    assert_eq!(provider.execute(41).await.unwrap(), 42);
}

#[test]
fn registry_resolves_lowest_priority_matching_tier() {
    let mut registry = Registry::new();
    registry.bind(Binding {
        operation_id: "embed".to_string(),
        provider: "basic",
        tiers: Vec::new(),
        priority: 10,
    });
    registry.bind(Binding {
        operation_id: "embed".to_string(),
        provider: "pro",
        tiers: vec!["pro".to_string()],
        priority: 1,
    });

    assert_eq!(*registry.resolve("embed", "free").unwrap(), "basic");
    assert_eq!(*registry.resolve("embed", "pro").unwrap(), "pro");
    assert_eq!(registry.list_bindings("embed").len(), 2);
}

use std::pin::Pin;

use futures::Stream as FuturesStream;
use rskit_errors::AppResult;

/// Boxed stream of `AppResult<O>`.
pub type BoxStream<O> = Pin<Box<dyn FuturesStream<Item = AppResult<O>> + Send + 'static>>;

// ─── Base ─────────────────────────────────────────────────────────────────────

/// Identity and availability for a provider.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Stable name identifying this provider instance (used in logs and spans).
    fn name(&self) -> &'static str;
}

// ─── Interaction patterns ─────────────────────────────────────────────────────

/// Unary request → single response (HTTP POST, gRPC unary, DB query).
#[async_trait::async_trait]
pub trait RequestResponse<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Execute the request and return a single response.
    async fn execute(&self, input: I) -> AppResult<O>;
}

/// One input → stream of outputs (gRPC server-stream, SSE, live query).
#[async_trait::async_trait]
pub trait Stream<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Open a stream of responses for the given input.
    async fn stream(&self, input: I) -> AppResult<BoxStream<O>>;
}

/// Write-only (Kafka publish, webhook, S3 put, log sink).
#[async_trait::async_trait]
pub trait Sink<I>: Provider
where
    I: Send + 'static,
{
    /// Send a single item downstream.
    async fn send(&self, input: I) -> AppResult<()>;
}

/// Bidirectional channel (WebSocket, gRPC bidi-stream).
#[async_trait::async_trait]
pub trait Duplex<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Open a new bidirectional channel.
    async fn open(&self) -> AppResult<Box<dyn DuplexChannel<I, O>>>;
}

/// Handle to an open bidirectional channel.
#[async_trait::async_trait]
pub trait DuplexChannel<I, O>: Send
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Send a message to the remote end.
    async fn send(&mut self, input: I) -> AppResult<()>;
    /// Receive the next message from the remote end; `None` means the channel is closed.
    async fn recv(&mut self) -> AppResult<Option<O>>;
    /// Close the channel gracefully.
    async fn close(&mut self) -> AppResult<()>;
}

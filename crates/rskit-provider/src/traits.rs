use std::future::Future;
use std::pin::Pin;

use futures::Stream;
use rskit_errors::AppResult;

/// Boxed stream of `AppResult<O>`.
pub type BoxStream<O> = Pin<Box<dyn Stream<Item = AppResult<O>> + Send + 'static>>;

// ─── Base ─────────────────────────────────────────────────────────────────────

/// Identity and availability for a provider.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Non-blocking availability check (e.g. connection pool not exhausted).
    async fn is_available(&self) -> bool {
        true
    }
}

// ─── Interaction patterns ─────────────────────────────────────────────────────

/// Unary request → single response (HTTP POST, gRPC unary, DB query).
#[async_trait::async_trait]
pub trait RequestResponse<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn execute(&self, input: I) -> AppResult<O>;
}

/// One input → stream of outputs (gRPC server-stream, SSE, live query).
#[async_trait::async_trait]
pub trait StreamProvider<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn stream(&self, input: I) -> AppResult<BoxStream<O>>;
}

/// Write-only (Kafka publish, webhook, S3 put, log sink).
#[async_trait::async_trait]
pub trait Sink<I>: Provider
where
    I: Send + 'static,
{
    async fn send(&self, input: I) -> AppResult<()>;
}

/// Bidirectional channel (WebSocket, gRPC bidi-stream).
#[async_trait::async_trait]
pub trait Duplex<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn open(&self) -> AppResult<Box<dyn DuplexChannel<I, O>>>;
}

/// Handle to an open bidirectional channel.
#[async_trait::async_trait]
pub trait DuplexChannel<I, O>: Send
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn send(&mut self, input: I) -> AppResult<()>;
    async fn recv(&mut self) -> AppResult<Option<O>>;
    async fn close(&mut self) -> AppResult<()>;
}

// ─── Lifecycle helpers (optional — for providers that need setup/teardown) ────

/// Providers that need async initialisation before first use.
#[async_trait::async_trait]
pub trait Initializable: Provider {
    async fn init(&self) -> AppResult<()>;
}

/// Providers that hold resources needing explicit cleanup.
#[async_trait::async_trait]
pub trait Closeable: Provider {
    async fn close(&self) -> AppResult<()>;
}

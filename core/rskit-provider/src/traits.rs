use std::future::Future;
use std::pin::Pin;

use futures::Stream as FuturesStream;
use rskit_errors::AppResult;

/// Boxed stream of `AppResult<O>`.
pub type BoxStream<O> = Pin<Box<dyn FuturesStream<Item = AppResult<O>> + Send + 'static>>;

// ─── Base ─────────────────────────────────────────────────────────────────────

/// Stable identity for a provider.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Stable name identifying this provider instance (used in logs and spans).
    fn name(&self) -> &'static str;
}

// ─── Interaction patterns ─────────────────────────────────────────────────────

/// Unary request → single response (HTTP POST, gRPC unary, DB query).
///
/// # Example
///
/// ```rust
/// use rskit_errors::AppResult;
/// use rskit_provider::{Provider, RequestResponse};
///
/// struct Echo;
///
/// impl Provider for Echo {
///     fn name(&self) -> &'static str {
///         "echo"
///     }
/// }
///
/// #[async_trait::async_trait]
/// impl RequestResponse<String, String> for Echo {
///     async fn execute(&self, input: String) -> AppResult<String> {
///         Ok(input.to_uppercase())
///     }
/// }
///
/// # #[tokio::main]
/// # async fn main() -> AppResult<()> {
/// let response = Echo.execute("hello".to_string()).await?;
/// assert_eq!(response, "HELLO");
/// # Ok(())
/// # }
/// ```
#[async_trait::async_trait]
pub trait RequestResponse<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Execute the request and return a single response.
    async fn execute(&self, input: I) -> AppResult<O>;
}

/// One input → bounded or backpressure-aware stream of outputs (gRPC server-stream, SSE, live query).
///
/// Implementations must document their buffering and backpressure behavior.
pub trait Stream<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Execute the request and return a stream of responses.
    fn execute(&self, input: I) -> impl Future<Output = AppResult<BoxStream<O>>> + Send + '_;
}

/// Write-only (Kafka publish, webhook, S3 put, log sink).
///
/// Implementations must apply downstream backpressure or return a typed error
/// instead of buffering without a bound.
pub trait Sink<I>: Provider
where
    I: Send + 'static,
{
    /// Send a single item downstream.
    fn send(&self, input: I) -> impl Future<Output = AppResult<()>> + Send + '_;
}

/// Bidirectional channel (WebSocket, gRPC bidi-stream).
///
/// Implementations must document channel buffering and close semantics.
pub trait Duplex<I, O>: Provider
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Open a new bidirectional channel.
    fn open(&self) -> impl Future<Output = AppResult<Box<dyn DuplexChannel<I, O>>>> + Send + '_;
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

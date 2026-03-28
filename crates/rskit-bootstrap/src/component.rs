use rskit_errors::AppResult;

use crate::Health;

/// Lifecycle-managed infrastructure component.
///
/// Implement this trait for databases, caches, gRPC servers, message queues —
/// any infrastructure that needs ordered start/stop and health reporting.
///
/// # Object safety
///
/// The `#[async_trait]` attribute makes this trait object-safe so components
/// can be stored as `Arc<dyn Component>` in the [`Registry`](crate::Registry).
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    /// Stable identifier used in logs and health responses.
    fn name(&self) -> &str;

    /// Start the component. Called before application hooks.
    async fn start(&self) -> AppResult<()>;

    /// Stop the component gracefully. Called after application stop hooks.
    async fn stop(&self) -> AppResult<()>;

    /// Instant (synchronous) health snapshot.
    fn health(&self) -> Health;
}

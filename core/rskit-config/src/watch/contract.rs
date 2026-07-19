use std::fmt;

use rskit_errors::AppResult;
use rskit_stream::BroadcastStream;
use tokio_util::sync::CancellationToken;

/// A typed configuration change emitted by a [`ConfigWatch`] source.
///
/// Carries only the affected key (never the value), so change notifications are safe to log.
/// Consumers react by re-running the load pipeline and re-decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigChange {
    /// The value at `key` was created or replaced.
    Set {
        /// The affected configuration key.
        key: String,
    },
    /// The value at `key` was removed.
    Removed {
        /// The affected configuration key.
        key: String,
    },
    /// The backend changed wholesale; consumers should reload everything.
    Reloaded,
}

/// A bounded stream of [`ConfigChange`] events.
///
/// The stream terminates when the originating [`CancellationToken`] fires or the source is dropped.
/// It is the [`Broadcaster`](rskit_stream::Broadcaster) stream specialized to config change events.
pub type ConfigChangeStream = BroadcastStream<ConfigChange>;

/// Adapter contract for configuration backends that emit change notifications.
///
/// Implemented by backends that can signal updates (a watched file, a remote config service, an in-memory store).
/// The returned stream is bounded and owned;
/// cancellation is driven by the supplied [`CancellationToken`] per the rskit concurrency baseline (ownership + cancellation + shutdown).
pub trait ConfigWatch: fmt::Debug + Send + Sync + 'static {
    /// Subscribe to configuration changes until `cancel` is triggered.
    ///
    /// The returned stream yields [`ConfigChange`] events and completes once `cancel` fires
    /// or the source is dropped.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`](rskit_errors::AppError) (cause preserved) if the backend cannot establish a watch.
    fn watch(&self, cancel: CancellationToken) -> AppResult<ConfigChangeStream>;
}

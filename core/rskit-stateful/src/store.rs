//! Store abstraction for accumulator backends.

use tokio::time::Instant;

use rskit_errors::AppResult;

/// Backend storage for an [`crate::Accumulator`].
pub trait Store<V>: Send + Sync
where
    V: Clone,
{
    /// Append a new value.
    fn append(&self, value: V) -> AppResult<()>;

    /// Remove and return the oldest value, if any.
    fn pop_oldest(&self) -> AppResult<Option<V>>;

    /// Snapshot all currently buffered values.
    fn snapshot(&self) -> AppResult<Vec<V>>;

    /// Drain and return all currently buffered values.
    fn flush(&self) -> AppResult<Vec<V>>;

    /// Return the current item count.
    fn size(&self) -> AppResult<usize>;

    /// Update last-activity metadata.
    fn touch(&self) -> AppResult<()>;

    /// Return the last activity timestamp.
    fn last_activity(&self) -> AppResult<Instant>;

    /// Release store resources.
    fn close(&self) -> AppResult<()>;
}

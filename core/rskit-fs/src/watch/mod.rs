//! Recursive, debounced filesystem-tree change watching.
//!
//! [`FsWatcher`](crate::watch::FsWatcher) observes one or more directory trees
//! and yields [`FsChangeBatch`](crate::watch::FsChangeBatch)es of changed paths,
//! collapsing bursts of raw events with a trailing-edge debounce window. It is the generic,
//! project-agnostic primitive behind edit→rebuild loops (test watchers, hot-reload tools, incremental planners):
//! the caller supplies the roots, a debounce duration,
//! and a [`CancellationToken`](rskit_stream::CancellationToken), and consumes an owned,
//! bounded [`FsChangeStream`](crate::watch::FsChangeStream).
//!
//! This is distinct from `rskit-config`'s reload watch, which observes a config *backend*
//! and emits keyed changes; this module observes a *path tree* and emits changed paths.
//! The whole module is gated behind the `watch` feature so one-shot tools never link `notify`.

mod change;
mod watcher;

pub use change::FsChangeBatch;
pub use watcher::{FsChangeStream, FsWatcher};

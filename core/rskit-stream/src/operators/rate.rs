//! Trailing-edge rate-limiting operators.
//!
//! These operators control *emission timing* rather than partitioning a stream
//! into fixed windows: their timers are **trailing-edge**, driven by quiet gaps
//! between arrivals (`debounce`, `debounce_batch`) or by a minimum spacing
//! between emissions (`throttle`). Contrast the leading-edge windowing
//! operators in [`crate::operators::windowing`], whose timers run from the first buffered
//! item.

use std::time::Duration;

use futures::{Stream, StreamExt as _};

use super::buffer::{take_window, window_capacity};

/// Emit only when no new item arrives within `delay`.
///
/// Useful for rate-limiting high-frequency event streams.
pub fn debounce<S, T>(stream: S, delay: Duration) -> impl Stream<Item = T> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut pending: Option<T> = None;

        loop {
            let has_pending = pending.is_some();
            tokio::select! {
                item = stream.next() => {
                    if let Some(v) = item {
                        pending = Some(v);
                    } else {
                        if let Some(v) = pending.take() { yield v; }
                        break;
                    }
                }
                () = async move {
                    if has_pending {
                        tokio::time::sleep(delay).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if let Some(v) = pending.take() { yield v; }
                }
            }
        }
    }
}

/// Accumulate items while they keep arriving, emitting the collected batch once
/// `quiet` elapses with no new item (trailing-edge, timer reset per item), or
/// early once `max_items` have accumulated.
///
/// This is [`debounce`] that keeps *every* item seen during the burst instead of
/// only the last: each arrival resets the quiet-window timer, and when the timer
/// finally fires the whole accumulated window is emitted as one `Vec<T>` in
/// arrival order. It differs from [`crate::operators::windowing::batch`] /
/// [`crate::operators::windowing::tumbling_window`], whose timers run from the *first* item
/// (leading-edge); here the timer is trailing-edge.
///
/// `max_items` (clamped to at least 1) is a safety cap that force-flushes the
/// window: because the timer resets on every item, a sustained input rate faster
/// than `quiet` would otherwise never reach a quiet window and grow the buffer
/// without bound. Reaching `max_items` emits the accumulated window immediately
/// and starts a fresh one — mirroring the size cap on the windowing operators.
pub fn debounce_batch<S, T>(
    stream: S,
    quiet: Duration,
    max_items: usize,
) -> impl Stream<Item = Vec<T>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let (max_items, initial_capacity) = window_capacity(max_items);
        let mut buf: Vec<T> = Vec::with_capacity(initial_capacity);
        // `None` parks the timer arm on `pending()` while the buffer is empty, so
        // a quiet stream never emits; the first item arms a trailing-edge deadline
        // that every subsequent item pushes further out.
        let mut deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                item = stream.next() => {
                    if let Some(v) = item {
                        buf.push(v);
                        deadline = Some(tokio::time::Instant::now() + quiet);
                        if buf.len() >= max_items {
                            deadline = None;
                            yield take_window(&mut buf, initial_capacity);
                        }
                    } else {
                        if !buf.is_empty() {
                            yield take_window(&mut buf, initial_capacity);
                        }
                        break;
                    }
                }
                () = async {
                    match deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    deadline = None;
                    if !buf.is_empty() {
                        yield take_window(&mut buf, initial_capacity);
                    }
                }
            }
        }
    }
}

/// Emit at most one item per `interval`, dropping faster arrivals.
pub fn throttle<S, T>(stream: S, interval: Duration) -> impl Stream<Item = T> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        // `None` until the first item is emitted, so the first arrival is always
        // yielded without a potentially-underflowing `now - interval`.
        let mut last_emit: Option<tokio::time::Instant> = None;
        while let Some(item) = stream.next().await {
            let now = tokio::time::Instant::now();
            let due = last_emit.is_none_or(|prev| now.duration_since(prev) >= interval);
            if due {
                last_emit = Some(now);
                yield item;
            }
        }
    }
}

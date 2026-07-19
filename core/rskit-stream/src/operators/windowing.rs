//! Leading-edge windowing and batching operators.
//!
//! These operators partition a stream into `Vec<T>` windows whose timers run **leading-edge**,
//! from the first buffered item (`tumbling_window`, `batch`), or by item count (`sliding_window`).
//! Contrast the trailing-edge rate operators in [`crate::operators::rate`],
//! whose timers are driven by quiet gaps between arrivals.

use std::collections::VecDeque;
use std::time::Duration;

use futures::{Stream, StreamExt as _};

use super::buffer::{take_window, window_capacity};

/// Collect items into non-overlapping time windows.
///
/// A window is emitted after `duration` has elapsed since the first item.
pub fn tumbling_window<S, T>(
    stream: S,
    duration: Duration,
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
        let mut deadline = tokio::time::Instant::now() + duration;
        loop {
            tokio::select! {
                item = stream.next() => {
                    if let Some(v) = item {
                        buf.push(v);
                        if buf.len() >= max_items {
                            yield take_window(&mut buf, initial_capacity);
                            deadline = tokio::time::Instant::now() + duration;
                        }
                    } else {
                        if !buf.is_empty() {
                            yield take_window(&mut buf, initial_capacity);
                        }
                        break;
                    }
                }
                () = tokio::time::sleep_until(deadline) => {
                    if !buf.is_empty() {
                        yield take_window(&mut buf, initial_capacity);
                    }
                    deadline = tokio::time::Instant::now() + duration;
                }
            }
        }
    }
}

/// Collect up to `size` items into a batch.
///
/// A batch is emitted either when `size` items arrive
/// or when `timeout` elapses since the first item in the batch.
pub fn batch<S, T>(
    stream: S,
    size: usize,
    timeout: Duration,
) -> impl Stream<Item = Vec<T>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let size = size.max(1);
        let mut buf: Vec<T> = Vec::with_capacity(size);
        let mut deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                item = stream.next() => {
                    if let Some(v) = item {
                        if buf.is_empty() {
                            deadline = Some(tokio::time::Instant::now() + timeout);
                        }
                        buf.push(v);
                        if buf.len() >= size {
                            deadline = None;
                            yield std::mem::take(&mut buf);
                        }
                    } else {
                        if !buf.is_empty() {
                            yield std::mem::take(&mut buf);
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
                        yield std::mem::take(&mut buf);
                    }
                }
            }
        }
    }
}

/// Emit sliding windows of `size` items, advancing by `step` items each time.
pub fn sliding_window<S, T>(
    stream: S,
    size: usize,
    step: usize,
) -> impl Stream<Item = Vec<T>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut window = VecDeque::with_capacity(size.max(1));
        let effective_step = step.max(1);

        while let Some(item) = stream.next().await {
            window.push_back(item);
            while window.len() > size {
                window.pop_front();
            }
            if size > 0 && window.len() == size {
                yield window.iter().cloned().collect();
                for _ in 0..effective_step.min(window.len()) {
                    window.pop_front();
                }
            }
        }
    }
}

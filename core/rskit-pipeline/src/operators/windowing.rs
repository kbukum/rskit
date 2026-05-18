use std::collections::VecDeque;
use std::time::Duration;

use futures::{Stream, StreamExt as _};

const TUMBLING_WINDOW_INITIAL_CAPACITY_LIMIT: usize = 1024;

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
        let max_items = max_items.max(1);
        let initial_capacity = max_items.min(TUMBLING_WINDOW_INITIAL_CAPACITY_LIMIT);
        let mut buf: Vec<T> = Vec::with_capacity(initial_capacity);
        let mut deadline = tokio::time::Instant::now() + duration;
        loop {
            tokio::select! {
                item = stream.next() => {
                    match item {
                        Some(v) => {
                            buf.push(v);
                            if buf.len() >= max_items {
                                yield take_window(&mut buf, initial_capacity);
                                deadline = tokio::time::Instant::now() + duration;
                            }
                        }
                        None => {
                            if !buf.is_empty() {
                                yield take_window(&mut buf, initial_capacity);
                            }
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if !buf.is_empty() {
                        yield take_window(&mut buf, initial_capacity);
                    }
                    deadline = tokio::time::Instant::now() + duration;
                }
            }
        }
    }
}

fn take_window<T>(buf: &mut Vec<T>, next_capacity: usize) -> Vec<T> {
    std::mem::replace(buf, Vec::with_capacity(next_capacity))
}

/// Collect up to `size` items into a batch.
///
/// A batch is emitted either when `size` items arrive or when `timeout`
/// elapses since the first item in the batch.
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
                    match item {
                        Some(v) => {
                            if buf.is_empty() {
                                deadline = Some(tokio::time::Instant::now() + timeout);
                            }
                            buf.push(v);
                            if buf.len() >= size {
                                deadline = None;
                                yield std::mem::take(&mut buf);
                            }
                        }
                        None => {
                            if !buf.is_empty() {
                                yield std::mem::take(&mut buf);
                            }
                            break;
                        }
                    }
                }
                _ = async {
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
                    match item {
                        Some(v) => { pending = Some(v); }
                        None => {
                            if let Some(v) = pending.take() { yield v; }
                            break;
                        }
                    }
                }
                _ = async move {
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

/// Emit at most one item per `interval`, dropping faster arrivals.
pub fn throttle<S, T>(stream: S, interval: Duration) -> impl Stream<Item = T> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut last_emit = tokio::time::Instant::now() - interval;
        while let Some(item) = stream.next().await {
            let now = tokio::time::Instant::now();
            if now.duration_since(last_emit) >= interval {
                last_emit = now;
                yield item;
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

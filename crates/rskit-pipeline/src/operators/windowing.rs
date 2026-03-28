use std::time::Duration;

use futures::Stream;

/// Collect items into non-overlapping time windows.
///
/// A window is emitted after `duration` has elapsed since the first item.
pub fn tumbling_window<S, T>(
    stream: S,
    duration: Duration,
) -> impl Stream<Item = Vec<T>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut buf: Vec<T> = Vec::new();
        let mut deadline = tokio::time::Instant::now() + duration;
        loop {
            tokio::select! {
                item = futures::StreamExt::next(&mut stream) => {
                    match item {
                        Some(v) => {
                            buf.push(v);
                        }
                        None => {
                            if !buf.is_empty() {
                                yield std::mem::take(&mut buf);
                            }
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if !buf.is_empty() {
                        yield std::mem::take(&mut buf);
                    }
                    deadline = tokio::time::Instant::now() + duration;
                }
            }
        }
    }
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
        let mut buf: Vec<T> = Vec::with_capacity(size);
        // Track deadline as Instant to avoid storing a !Unpin Sleep future.
        let mut deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                item = futures::StreamExt::next(&mut stream) => {
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

/// Only emit an item if no new item arrives within `delay`.
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
                item = futures::StreamExt::next(&mut stream) => {
                    match item {
                        Some(v) => { pending = Some(v); }
                        None => {
                            if let Some(v) = pending.take() { yield v; }
                            break;
                        }
                    }
                }
                _ = async move {
                    // Capture only a bool — avoids borrowing &Option<T> which
                    // would require T: Sync for the async block to be Send.
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
        while let Some(item) = futures::StreamExt::next(&mut stream).await {
            let now = tokio::time::Instant::now();
            if now.duration_since(last_emit) >= interval {
                last_emit = now;
                yield item;
            }
        }
    }
}

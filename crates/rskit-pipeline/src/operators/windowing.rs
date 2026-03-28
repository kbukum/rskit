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
        let mut deadline: Option<tokio::time::Sleep> = None;

        loop {
            // Build a future that fires when the deadline is set
            let timer_expired = async {
                match &mut deadline {
                    Some(sleep) => {
                        std::future::poll_fn(|cx| {
                            use std::pin::Pin;
                            Pin::new(sleep).poll(cx)
                        }).await
                    }
                    None => std::future::pending().await,
                }
            };

            tokio::select! {
                item = futures::StreamExt::next(&mut stream) => {
                    match item {
                        Some(v) => {
                            if buf.is_empty() {
                                deadline = Some(Box::pin(tokio::time::sleep(timeout)));
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
                _ = timer_expired => {
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
pub fn debounce<S, T>(
    stream: S,
    delay: Duration,
) -> impl Stream<Item = T> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut pending: Option<T> = None;

        loop {
            let timer = async {
                if pending.is_some() {
                    tokio::time::sleep(delay).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };

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
                _ = timer => {
                    if let Some(v) = pending.take() { yield v; }
                }
            }
        }
    }
}

/// Emit at most one item per `interval`, dropping faster arrivals.
pub fn throttle<S, T>(
    stream: S,
    interval: Duration,
) -> impl Stream<Item = T> + Send + 'static
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

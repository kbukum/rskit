use futures::StreamExt;
use rskit_errors::AppResult;

/// Consume the stream and collect all items into a `Vec`.
pub async fn collect<T, S: futures::Stream<Item = T> + Unpin>(stream: S) -> Vec<T> {
    stream.collect().await
}

/// Apply `f` to every item, stopping on first error.
pub async fn for_each<T, S, F, Fut>(mut stream: S, mut f: F) -> AppResult<()>
where
    S: futures::Stream<Item = T> + Unpin,
    F: FnMut(T) -> Fut,
    Fut: std::future::Future<Output = AppResult<()>>,
{
    while let Some(item) = stream.next().await {
        f(item).await?;
    }
    Ok(())
}

/// Drain the stream into `sink`, forwarding each item without stopping on error.
pub async fn drain<T, S, F, Fut>(mut stream: S, mut sink: F) -> AppResult<()>
where
    S: futures::Stream<Item = AppResult<T>> + Unpin,
    F: FnMut(T) -> Fut,
    Fut: std::future::Future<Output = AppResult<()>>,
{
    while let Some(item) = stream.next().await {
        sink(item?).await?;
    }
    Ok(())
}

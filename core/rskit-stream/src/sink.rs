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

#[cfg(test)]
mod tests {
    use futures::stream;
    use rskit_errors::{AppError, ErrorCode};

    use super::*;

    #[tokio::test]
    async fn collect_for_each_and_drain_cover_success_and_error_paths() {
        assert_eq!(collect(stream::iter([1, 2, 3])).await, vec![1, 2, 3]);

        let mut seen = Vec::new();
        for_each(stream::iter([1, 2]), |item| {
            seen.push(item);
            async { Ok(()) }
        })
        .await
        .unwrap();
        assert_eq!(seen, vec![1, 2]);

        let error = for_each(stream::iter([1]), |_item| async {
            Err(AppError::new(ErrorCode::InvalidInput, "bad item"))
        })
        .await
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);

        let mut drained = Vec::new();
        drain(stream::iter([Ok(4), Ok(5)]), |item| {
            drained.push(item);
            async { Ok(()) }
        })
        .await
        .unwrap();
        assert_eq!(drained, vec![4, 5]);

        let error = drain(
            stream::iter([Err::<u32, _>(AppError::new(ErrorCode::Internal, "source"))]),
            |_item| async { Ok(()) },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Internal);
    }
}

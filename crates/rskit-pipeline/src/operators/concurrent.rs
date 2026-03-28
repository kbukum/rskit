use futures::Stream;
use rskit_errors::AppResult;
use std::future::Future;

/// Process up to `concurrency` items concurrently.
///
/// Output order is not preserved (items complete as they finish).
/// Uses `futures::stream::FuturesUnordered` for zero-copy task tracking.
pub fn parallel<S, T, O, F, Fut>(
    stream: S,
    concurrency: usize,
    f: F,
) -> impl Stream<Item = AppResult<O>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
    O: Send + 'static,
    F: Fn(T) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    use futures::StreamExt;
    stream.map(move |item| f(item)).buffer_unordered(concurrency)
}

/// Apply multiple functions to the same item concurrently, collecting results.
///
/// All functions are applied to a clone of each item; all results are
/// collected into a `Vec` in the order of `fns`.
pub fn fan_out<S, T, O, F, Fut>(
    stream: S,
    fns: Vec<F>,
) -> impl Stream<Item = Vec<AppResult<O>>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Clone + Send + 'static,
    O: Send + 'static,
    F: Fn(T) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    use futures::StreamExt;
    let fns = std::sync::Arc::new(fns);
    stream.then(move |item| {
        let fns = fns.clone();
        async move {
            let handles: Vec<_> = fns.iter()
                .map(|f| {
                    let fut = f(item.clone());
                    tokio::spawn(fut)
                })
                .collect();
            let mut results = Vec::with_capacity(handles.len());
            for h in handles {
                results.push(h.await.unwrap_or_else(|e| {
                    Err(rskit_errors::AppError::internal(e))
                }));
            }
            results
        }
    })
}

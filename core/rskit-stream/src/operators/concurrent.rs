use futures::Stream;
use rskit_errors::{AppError, AppResult};
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
    if concurrency == 0 {
        futures::stream::once(async {
            Err(AppError::invalid_input(
                "concurrency",
                "must be greater than zero",
            ))
        })
        .left_stream()
    } else {
        stream.map(f).buffer_unordered(concurrency).right_stream()
    }
}

/// Apply multiple functions to the same item concurrently, collecting results.
///
/// All functions are applied to a clone of each item;
/// all results are collected into a `Vec` in the order of `fns`.
/// Branch futures are polled concurrently by the stream task
/// and short-circuit on the first branch error.
pub fn fan_out<S, T, O, F, Fut>(
    stream: S,
    max_branches: usize,
    fns: Vec<F>,
) -> impl Stream<Item = AppResult<Vec<O>>> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Clone + Send + 'static,
    O: Send + 'static,
    F: Fn(T) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    use futures::StreamExt;
    if max_branches == 0 || fns.len() > max_branches {
        let error = AppError::invalid_input(
            "max_branches",
            format!("branch count {} exceeds limit {max_branches}", fns.len()),
        );
        futures::stream::once(async { Err(error) }).left_stream()
    } else {
        let fns = std::sync::Arc::new(fns);
        stream
            .then(move |item| {
                let fns = fns.clone();
                async move { futures::future::try_join_all(fns.iter().map(|f| f(item.clone()))).await }
            })
            .right_stream()
    }
}

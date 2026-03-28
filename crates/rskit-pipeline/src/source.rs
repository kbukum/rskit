use futures_util::stream;
use tokio::sync::mpsc;

/// Create a stream from a `Vec<T>`.
pub fn from_slice<T: Send + 'static>(
    items: Vec<T>,
) -> impl futures::Stream<Item = T> + Send + 'static {
    stream::iter(items)
}

/// Create a lazy stream from an async generator function.
///
/// `f` is called repeatedly; it should return `Some(item)` while there
/// are items, and `None` to terminate.
pub fn from_fn<T, F, Fut>(mut f: F) -> impl futures::Stream<Item = T> + Send + 'static
where
    T: Send + 'static,
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Option<T>> + Send + 'static,
{
    stream::unfold((), move |()| {
        let fut = f();
        async move { fut.await.map(|v| (v, ())) }
    })
}

/// Wrap a `tokio::sync::mpsc::Receiver` as a `Stream`.
pub fn from_channel<T: Send + 'static>(
    rx: mpsc::Receiver<T>,
) -> impl futures::Stream<Item = T> + Send + 'static {
    tokio_stream::wrappers::ReceiverStream::new(rx)
}

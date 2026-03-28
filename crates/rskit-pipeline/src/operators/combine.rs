use futures::StreamExt;

/// Merge two streams, yielding items from whichever is ready first.
pub fn merge<T: Send + 'static>(
    s1: impl futures::Stream<Item = T> + Send + 'static,
    s2: impl futures::Stream<Item = T> + Send + 'static,
) -> impl futures::Stream<Item = T> + Send + 'static {
    futures::stream::select(s1, s2)
}

/// Concatenate a list of streams — exhausts each before moving to the next.
pub fn concat<T: Send + 'static>(
    streams: Vec<impl futures::Stream<Item = T> + Send + 'static>,
) -> impl futures::Stream<Item = T> + Send + 'static {
    futures::stream::iter(streams).flatten()
}

use std::collections::HashSet;
use std::hash::Hash;

use futures::{Stream, StreamExt as _};
use tokio::sync::mpsc;

use crate::source;

/// Emit only the first occurrence of each item.
pub fn distinct<S, T>(stream: S) -> impl Stream<Item = T> + Send + 'static
where
    S: Stream<Item = T> + Send + 'static,
    T: Clone + Eq + Hash + Send + 'static,
{
    async_stream::stream! {
        tokio::pin!(stream);
        let mut seen = HashSet::new();
        while let Some(item) = stream.next().await {
            if seen.insert(item.clone()) {
                yield item;
            }
        }
    }
}

/// Split a stream into two streams based on `predicate`.
pub fn partition<S, T, F>(
    stream: S,
    mut predicate: F,
) -> (
    impl Stream<Item = T> + Send + 'static,
    impl Stream<Item = T> + Send + 'static,
)
where
    S: Stream<Item = T> + Send + 'static,
    T: Send + 'static,
    F: FnMut(&T) -> bool + Send + 'static,
{
    let (left_tx, left_rx) = mpsc::channel(64);
    let (right_tx, right_rx) = mpsc::channel(64);

    tokio::spawn(async move {
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            let sender = if predicate(&item) {
                &left_tx
            } else {
                &right_tx
            };
            if sender.send(item).await.is_err() {
                break;
            }
        }
    });

    (
        source::from_channel(left_rx),
        source::from_channel(right_rx),
    )
}

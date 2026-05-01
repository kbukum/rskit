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
        let mut left_open = true;
        let mut right_open = true;

        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            if !left_open && !right_open {
                break;
            }

            if predicate(&item) {
                if left_open && left_tx.send(item).await.is_err() {
                    left_open = false;
                }
            } else if right_open && right_tx.send(item).await.is_err() {
                right_open = false;
            }
        }
    });

    (
        source::from_channel(left_rx),
        source::from_channel(right_rx),
    )
}

//! [`RskitStreamExt`] — ergonomic extension methods on [`futures::Stream`].

use std::future::Future;
use std::hash::Hash;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt as _;
use rskit_errors::AppResult;

use crate::operators::{concurrent, rate, transform, windowing};

/// Extension trait adding rskit-specific operators to any [`Stream`].
///
/// Imported with `use rskit_stream::RskitStreamExt;`.
#[allow(async_fn_in_trait)]
pub trait RskitStreamExt: Stream + Sized + Send + 'static
where
    Self::Item: Send + 'static,
{
    /// Async map — apply a fallible async function to each item.
    fn rmap<O, F, Fut>(self, f: F) -> impl Stream<Item = AppResult<O>> + Send + 'static
    where
        O: Send + 'static,
        F: FnMut(Self::Item) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        self.then(f)
    }

    /// Async flat-map — apply a function that returns a stream and flatten one level.
    fn rflatmap<O, F, Fut, St>(self, f: F) -> impl Stream<Item = AppResult<O>> + Send + 'static
    where
        O: Send + 'static,
        F: FnMut(Self::Item) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<St>> + Send + 'static,
        St: Stream<Item = AppResult<O>> + Send + Unpin + 'static,
    {
        self.then(f).flat_map(|result| match result {
            Ok(s) => s.left_stream(),
            Err(e) => futures::stream::once(futures::future::ready(Err(e))).right_stream(),
        })
    }

    /// Keep only items that satisfy the (synchronous) predicate.
    fn rfilter<F>(self, mut f: F) -> impl Stream<Item = Self::Item> + Send + 'static
    where
        F: FnMut(&Self::Item) -> bool + Send + 'static,
    {
        self.filter(move |item| {
            let keep = f(item);
            std::future::ready(keep)
        })
    }

    /// Emit only the first occurrence of each item.
    fn rdistinct(self) -> impl Stream<Item = Self::Item> + Send + 'static
    where
        Self::Item: Clone + Eq + Hash,
    {
        transform::distinct(self)
    }

    /// Side-effect for each item — does not modify the stream.
    fn rtap<F, Fut>(self, mut f: F) -> impl Stream<Item = Self::Item> + Send + 'static
    where
        F: FnMut(&Self::Item) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.then(move |item| {
            let fut = f(&item);
            async move {
                fut.await;
                item
            }
        })
    }

    /// Take the first `n` items from the stream.
    fn rtake(self, n: usize) -> impl Stream<Item = Self::Item> + Send + 'static {
        self.take(n)
    }

    /// Skip the first `n` items from the stream.
    fn rskip(self, n: usize) -> impl Stream<Item = Self::Item> + Send + 'static {
        self.skip(n)
    }

    /// Split the stream into `(matching, remainder)` streams.
    fn rpartition<F>(
        self,
        predicate: F,
    ) -> (
        impl Stream<Item = Self::Item> + Send + 'static,
        impl Stream<Item = Self::Item> + Send + 'static,
    )
    where
        F: FnMut(&Self::Item) -> bool + Send + 'static,
    {
        transform::partition(self, predicate)
    }

    /// Fold the entire stream into a single value.
    async fn rreduce<Acc, F>(self, init: Acc, mut f: F) -> Acc
    where
        Acc: Send + 'static,
        F: FnMut(Acc, Self::Item) -> Acc + Send + 'static,
    {
        let mut acc = init;
        let this = self;
        tokio::pin!(this);
        while let Some(item) = this.next().await {
            acc = f(acc, item);
        }
        acc
    }

    /// Process up to `concurrency` items in parallel (unordered output).
    fn rparallel<O, F, Fut>(
        self,
        concurrency: usize,
        f: F,
    ) -> impl Stream<Item = AppResult<O>> + Send + 'static
    where
        O: Send + 'static,
        F: Fn(Self::Item) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        concurrent::parallel(self, concurrency, f)
    }

    /// Apply multiple functions to the same item and collect all results.
    fn rfan_out<O, F, Fut>(
        self,
        max_branches: usize,
        fns: Vec<F>,
    ) -> impl Stream<Item = AppResult<Vec<O>>> + Send + 'static
    where
        O: Send + 'static,
        Self::Item: Clone,
        F: Fn(Self::Item) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        concurrent::fan_out(self, max_branches, fns)
    }

    /// Collect items into non-overlapping windows bounded by time and item count.
    fn rtumbling_window(
        self,
        duration: Duration,
        max_items: usize,
    ) -> impl Stream<Item = Vec<Self::Item>> + Send + 'static {
        windowing::tumbling_window(self, duration, max_items)
    }

    /// Emit sliding windows of `size` items, advancing by `step` items each time.
    fn rsliding_window(
        self,
        size: usize,
        step: usize,
    ) -> impl Stream<Item = Vec<Self::Item>> + Send + 'static
    where
        Self::Item: Clone,
    {
        windowing::sliding_window(self, size, step)
    }

    /// Emit batches of up to `size` items or when `timeout` elapses.
    fn rbatch(
        self,
        size: usize,
        timeout: Duration,
    ) -> impl Stream<Item = Vec<Self::Item>> + Send + 'static {
        windowing::batch(self, size, timeout)
    }

    /// Emit only when no new item arrives within `delay`.
    fn rdebounce(self, delay: Duration) -> impl Stream<Item = Self::Item> + Send + 'static {
        rate::debounce(self, delay)
    }

    /// Accumulate items into a batch,
    /// emitting it once `quiet` elapses with no new item (trailing-edge debounce that keeps every item, not just the last)
    /// or early once `max_items` accumulate.
    fn rdebounce_batch(
        self,
        quiet: Duration,
        max_items: usize,
    ) -> impl Stream<Item = Vec<Self::Item>> + Send + 'static {
        rate::debounce_batch(self, quiet, max_items)
    }

    /// Emit at most one item per `interval`.
    fn rthrottle(self, interval: Duration) -> impl Stream<Item = Self::Item> + Send + 'static {
        rate::throttle(self, interval)
    }

    /// Merge this stream with another, yielding items from whichever is ready first.
    fn rmerge(
        self,
        other: impl Stream<Item = Self::Item> + Send + 'static,
    ) -> impl Stream<Item = Self::Item> + Send + 'static {
        futures::stream::select(self, other)
    }
}

impl<S> RskitStreamExt for S
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
{
}

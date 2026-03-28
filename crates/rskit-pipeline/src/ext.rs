//! [`RskitStreamExt`] — ergonomic extension methods on [`futures::Stream`].

use std::future::Future;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt as _;
use rskit_errors::AppResult;

use crate::operators::{concurrent, windowing};

/// Extension trait adding rskit-specific operators to any [`Stream`].
///
/// Imported with `use rskit_pipeline::RskitStreamExt;`.
pub trait RskitStreamExt: Stream + Sized + Send + 'static
where
    Self::Item: Send + 'static,
{
    // ── Transform ─────────────────────────────────────────────────────

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
            Err(e) => {
                futures::stream::once(futures::future::ready(Err(e))).right_stream()
            }
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

    /// Fold the entire stream into a single value.
    async fn rreduce<Acc, F>(self, init: Acc, mut f: F) -> Acc
    where
        Acc: Send + 'static,
        F: FnMut(Acc, Self::Item) -> Acc + Send + 'static,
    {
        let mut acc = init;
        // `self` can't be used with `tokio::pin!` — bind to a named variable first.
        let mut this = self;
        tokio::pin!(this);
        while let Some(item) = this.next().await {
            acc = f(acc, item);
        }
        acc
    }

    // ── Concurrency ───────────────────────────────────────────────────

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
        fns: Vec<F>,
    ) -> impl Stream<Item = Vec<AppResult<O>>> + Send + 'static
    where
        O: Send + 'static,
        Self::Item: Clone,
        F: Fn(Self::Item) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        concurrent::fan_out(self, fns)
    }

    // ── Windowing / time ──────────────────────────────────────────────

    /// Collect items into fixed-duration non-overlapping windows.
    fn rtumbling_window(
        self,
        duration: Duration,
    ) -> impl Stream<Item = Vec<Self::Item>> + Send + 'static {
        windowing::tumbling_window(self, duration)
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
        windowing::debounce(self, delay)
    }

    /// Emit at most one item per `interval`.
    fn rthrottle(self, interval: Duration) -> impl Stream<Item = Self::Item> + Send + 'static {
        windowing::throttle(self, interval)
    }
}

/// Blanket implementation for all compatible streams.
impl<S> RskitStreamExt for S
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
{
}

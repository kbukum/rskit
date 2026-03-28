//! Composable async data pipelines built on `futures::Stream`.

#![warn(missing_docs)]

/// Extension trait adding `rskit` operators to any `Stream`.
pub mod ext;
/// Higher-level stream operators (map, filter, fan-out, windowing, etc.).
pub mod operators;
/// Terminal sink combinators (`collect`, `drain`, `for_each`).
pub mod sink;
/// Stream source constructors (`from_slice`, `from_fn`, `from_channel`).
pub mod source;

pub use ext::RskitStreamExt;
pub use operators::combine::{concat, merge};
pub use sink::{collect, drain, for_each};
pub use source::{from_channel, from_fn, from_slice};

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use futures::StreamExt as _;

    use crate::{RskitStreamExt, from_fn, from_slice, merge};

    // ── Sources ───────────────────────────────────────────────────────────

    /// `from_slice` must yield every item in the original order.
    #[tokio::test]
    async fn test_from_slice_yields_all_in_order() {
        let items = vec![1u32, 2, 3, 4, 5];
        let stream = from_slice(items.clone());
        let collected: Vec<u32> = stream.collect().await;
        assert_eq!(collected, items);
    }

    /// `from_slice` with an empty vec yields nothing.
    #[tokio::test]
    async fn test_from_slice_empty() {
        let stream = from_slice::<u32>(vec![]);
        let collected: Vec<u32> = stream.collect().await;
        assert!(collected.is_empty());
    }

    /// `from_fn` calls the function repeatedly and stops when it returns `None`.
    #[tokio::test]
    async fn test_from_fn_yields_until_none() {
        let counter = Arc::new(Mutex::new(0u32));
        let c = counter.clone();
        let stream = from_fn(move || {
            let c = c.clone();
            async move {
                let mut n = c.lock().unwrap();
                if *n < 5 {
                    let val = *n;
                    *n += 1;
                    Some(val)
                } else {
                    None
                }
            }
        });
        let collected: Vec<u32> = stream.collect().await;
        assert_eq!(collected, vec![0, 1, 2, 3, 4]);
    }

    /// `from_fn` that immediately returns `None` yields nothing.
    #[tokio::test]
    async fn test_from_fn_immediate_none() {
        let stream = from_fn(|| async { None::<u32> });
        let collected: Vec<u32> = stream.collect().await;
        assert!(collected.is_empty());
    }

    /// `merge` interleaves two streams; the combined set of items must match.
    #[tokio::test]
    async fn test_merge_set_equality() {
        let s1 = from_slice(vec![1u32, 3, 5]);
        let s2 = from_slice(vec![2u32, 4, 6]);
        let mut combined: Vec<u32> = merge(s1, s2).collect().await;
        combined.sort();
        assert_eq!(combined, vec![1, 2, 3, 4, 5, 6]);
    }

    /// `merge` of two empty streams yields nothing.
    #[tokio::test]
    async fn test_merge_both_empty() {
        let s1 = from_slice::<u32>(vec![]);
        let s2 = from_slice::<u32>(vec![]);
        let combined: Vec<u32> = merge(s1, s2).collect().await;
        assert!(combined.is_empty());
    }

    // ── RskitStreamExt::rmap ──────────────────────────────────────────────

    /// `rmap` transforms each item via an async fallible function.
    #[tokio::test]
    async fn test_rmap_transforms_items() {
        let stream = from_slice(vec![1u32, 2, 3]);
        let results: Vec<_> = stream
            .rmap(|x| async move { Ok::<u32, rskit_errors::AppError>(x * 10) })
            .collect()
            .await;
        let values: Vec<u32> = results.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(values, vec![10, 20, 30]);
    }

    /// `rmap` propagates errors returned by the function.
    #[tokio::test]
    async fn test_rmap_propagates_error() {
        let stream = from_slice(vec![1u32, 2, 3]);
        let results: Vec<_> = stream
            .rmap(|x| async move {
                if x == 2 {
                    Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::Internal,
                        "bad item",
                    ))
                } else {
                    Ok(x)
                }
            })
            .collect()
            .await;
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
    }

    // ── RskitStreamExt::rfilter ───────────────────────────────────────────

    /// `rfilter` keeps only items satisfying the predicate.
    #[tokio::test]
    async fn test_rfilter_keeps_matching_items() {
        let stream = from_slice(vec![1u32, 2, 3, 4, 5, 6]);
        let evens: Vec<u32> = stream.rfilter(|x| x % 2 == 0).collect().await;
        assert_eq!(evens, vec![2, 4, 6]);
    }

    /// `rfilter` with a predicate that matches nothing yields an empty stream.
    #[tokio::test]
    async fn test_rfilter_no_match_yields_empty() {
        let stream = from_slice(vec![1u32, 3, 5]);
        let result: Vec<u32> = stream.rfilter(|x| x % 2 == 0).collect().await;
        assert!(result.is_empty());
    }

    // ── RskitStreamExt::rtap ──────────────────────────────────────────────

    /// `rtap` calls the side-effect for every item and passes items through unchanged.
    #[tokio::test]
    async fn test_rtap_calls_side_effect_and_passes_through() {
        let seen = Arc::new(Mutex::new(Vec::<u32>::new()));
        let seen_clone = seen.clone();

        let stream = from_slice(vec![10u32, 20, 30]);
        let output: Vec<u32> = stream
            .rtap(move |x| {
                let seen = seen_clone.clone();
                let val = *x;
                async move {
                    seen.lock().unwrap().push(val);
                }
            })
            .collect()
            .await;

        assert_eq!(output, vec![10, 20, 30]);
        assert_eq!(*seen.lock().unwrap(), vec![10, 20, 30]);
    }

    // ── RskitStreamExt::rreduce ───────────────────────────────────────────

    /// `rreduce` folds the entire stream into a single accumulated value.
    #[tokio::test]
    async fn test_rreduce_folds_to_single_value() {
        let stream = from_slice(vec![1u32, 2, 3, 4, 5]);
        let sum = stream.rreduce(0u32, |acc, x| acc + x).await;
        assert_eq!(sum, 15);
    }

    /// `rreduce` on an empty stream returns the initial accumulator.
    #[tokio::test]
    async fn test_rreduce_empty_stream_returns_init() {
        let stream = from_slice::<u32>(vec![]);
        let result = stream.rreduce(42u32, |acc, x| acc + x).await;
        assert_eq!(result, 42);
    }

    // ── RskitStreamExt::rparallel ─────────────────────────────────────────

    /// `rparallel` processes items concurrently and collects all results.
    #[tokio::test]
    async fn test_rparallel_collects_all_results() {
        let stream = from_slice(vec![1u32, 2, 3, 4, 5]);
        let mut results: Vec<u32> = stream
            .rparallel(
                3,
                |x| async move { Ok::<u32, rskit_errors::AppError>(x * 2) },
            )
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        results.sort();
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
    }

    /// `rparallel` propagates errors from the worker function.
    #[tokio::test]
    async fn test_rparallel_propagates_errors() {
        let stream = from_slice(vec![1u32, 2, 3]);
        let results: Vec<_> = stream
            .rparallel(2, |x| async move {
                if x == 2 {
                    Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::Internal,
                        "parallel error",
                    ))
                } else {
                    Ok(x)
                }
            })
            .collect()
            .await;
        let errors: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
        assert_eq!(errors.len(), 1);
    }

    // ── RskitStreamExt::rfan_out ──────────────────────────────────────────

    /// `rfan_out` applies N functions to each item and collects results in order.
    ///
    /// We use non-capturing closures (which are Copy + Clone) so the
    /// `F: Clone` bound on `rfan_out` is satisfied without unstable features.
    #[tokio::test]
    async fn test_rfan_out_applies_all_functions() {
        // Non-capturing closures are Copy, so they satisfy Clone.
        let add_one = |x: u32| std::future::ready(Ok::<u32, rskit_errors::AppError>(x + 1));
        let mul_two = |x: u32| std::future::ready(Ok::<u32, rskit_errors::AppError>(x * 2));

        // First check: single add_one function
        let stream_a = from_slice(vec![5u32, 10u32]);
        let res_a: Vec<Vec<_>> = stream_a.rfan_out(vec![add_one]).collect().await;
        assert_eq!(*res_a[0][0].as_ref().unwrap(), 6u32);
        assert_eq!(*res_a[1][0].as_ref().unwrap(), 11u32);

        // Second check: two homogeneous functions of the same concrete type
        let stream_b = from_slice(vec![5u32, 10u32]);
        let res_b: Vec<Vec<_>> = stream_b.rfan_out(vec![add_one, mul_two]).collect().await;
        // item 5  → [5+1=6, 5*2=10]
        assert_eq!(*res_b[0][0].as_ref().unwrap(), 6u32);
        assert_eq!(*res_b[0][1].as_ref().unwrap(), 10u32);
        // item 10 → [10+1=11, 10*2=20]
        assert_eq!(*res_b[1][0].as_ref().unwrap(), 11u32);
        assert_eq!(*res_b[1][1].as_ref().unwrap(), 20u32);
    }

    /// `rfan_out` with a single function behaves like rmap.
    #[tokio::test]
    async fn test_rfan_out_single_function() {
        let stream = from_slice(vec![3u32, 7u32]);
        // Non-capturing closure is Copy + Clone.
        let f = |x: u32| std::future::ready(Ok::<u32, rskit_errors::AppError>(x + 100));
        let results: Vec<Vec<_>> = stream.rfan_out(vec![f]).collect().await;
        assert_eq!(results.len(), 2);
        assert_eq!(*results[0][0].as_ref().unwrap(), 103u32);
        assert_eq!(*results[1][0].as_ref().unwrap(), 107u32);
    }

    // ── Windowing: rbatch ─────────────────────────────────────────────────

    /// `rbatch` with size=3 produces batches of exactly 3 items when enough arrive.
    #[tokio::test]
    async fn test_rbatch_exact_size_batches() {
        tokio::time::pause();

        let stream = from_slice(vec![1u32, 2, 3, 4, 5, 6]);
        let handle = tokio::spawn(async move {
            stream
                .rbatch(3, Duration::from_millis(500))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(Duration::from_millis(600)).await;
        let batches = handle.await.unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], vec![1, 2, 3]);
        assert_eq!(batches[1], vec![4, 5, 6]);
    }

    /// `rbatch` flushes a partial batch on timeout.
    #[tokio::test]
    async fn test_rbatch_partial_flush_on_timeout() {
        tokio::time::pause();

        // Channel-based stream so we can control item arrival timing.
        let (tx, rx) = tokio::sync::mpsc::channel::<u32>(16);
        let stream = crate::source::from_channel(rx);

        let handle = tokio::spawn(async move {
            stream
                .rbatch(10, Duration::from_millis(100))
                .collect::<Vec<_>>()
                .await
        });

        // Send 2 items then let the timeout fire.
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        drop(tx); // close channel after items sent

        tokio::time::advance(Duration::from_millis(200)).await;
        let batches = handle.await.unwrap();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![1, 2]);
    }

    // ── Windowing: rdebounce ──────────────────────────────────────────────

    /// `rdebounce` only emits the last item when the quiet window expires.
    #[tokio::test]
    async fn test_rdebounce_emits_last_item() {
        tokio::time::pause();

        let (tx, rx) = tokio::sync::mpsc::channel::<u32>(16);
        let stream = crate::source::from_channel(rx);

        let handle = tokio::spawn(async move {
            stream
                .rdebounce(Duration::from_millis(100))
                .collect::<Vec<_>>()
                .await
        });

        // Three rapid items — only the last should pass through.
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        tx.send(3).await.unwrap();
        drop(tx);

        tokio::time::advance(Duration::from_millis(200)).await;
        let result = handle.await.unwrap();

        // After the channel closes, the pending item must be flushed.
        assert!(!result.is_empty());
        assert_eq!(*result.last().unwrap(), 3u32);
    }

    // ── Windowing: rthrottle ──────────────────────────────────────────────

    /// `rthrottle` drops items arriving faster than the interval.
    #[tokio::test]
    async fn test_rthrottle_drops_fast_items() {
        tokio::time::pause();

        let stream = from_slice(vec![1u32, 2, 3, 4, 5]);
        let handle = tokio::spawn(async move {
            stream
                .rthrottle(Duration::from_millis(100))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(Duration::from_millis(600)).await;
        let result = handle.await.unwrap();

        // The first item is always emitted; subsequent items are dropped
        // because the stream is synchronous and all items arrive "instantly"
        // before the interval can pass.
        assert!(!result.is_empty());
        assert_eq!(result[0], 1u32);
        // All items after the first should have been throttled away.
        assert!(result.len() < 5);
    }

    // ── Windowing: rtumbling_window ───────────────────────────────────────

    /// `rtumbling_window` emits a non-empty window when the timer fires.
    #[tokio::test]
    async fn test_rtumbling_window_emits_on_timer() {
        tokio::time::pause();

        let (tx, rx) = tokio::sync::mpsc::channel::<u32>(16);
        let stream = crate::source::from_channel(rx);

        let handle = tokio::spawn(async move {
            stream
                .rtumbling_window(Duration::from_millis(100))
                .collect::<Vec<_>>()
                .await
        });

        // Send items that should land in the first window.
        tx.send(10).await.unwrap();
        tx.send(20).await.unwrap();
        tx.send(30).await.unwrap();
        drop(tx);

        tokio::time::advance(Duration::from_millis(200)).await;
        let windows = handle.await.unwrap();

        assert!(!windows.is_empty());
        let all_items: Vec<u32> = windows.into_iter().flatten().collect();
        let mut sorted = all_items.clone();
        sorted.sort();
        assert_eq!(sorted, vec![10, 20, 30]);
    }

    /// `rtumbling_window` yields an empty stream when input is empty.
    #[tokio::test]
    async fn test_rtumbling_window_empty_input() {
        tokio::time::pause();

        let stream = from_slice::<u32>(vec![]);
        let handle = tokio::spawn(async move {
            stream
                .rtumbling_window(Duration::from_millis(100))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(Duration::from_millis(200)).await;
        let windows = handle.await.unwrap();
        assert!(windows.is_empty());
    }
}

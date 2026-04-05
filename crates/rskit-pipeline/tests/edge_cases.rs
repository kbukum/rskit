use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_pipeline::{concat, from_slice, merge, RskitStreamExt};

// ── 1. Empty stream through rmap ──────────────────────────────────────────

#[tokio::test]
async fn test_empty_stream_rmap() {
    let results: Vec<AppResult<u32>> = from_slice::<u32>(vec![])
        .rmap(|x| async move { Ok(x * 10) })
        .collect::<Vec<_>>()
        .await;
    assert!(results.is_empty());
}

// ── 2. Empty stream through rbatch ────────────────────────────────────────

#[tokio::test]
async fn test_empty_stream_rbatch() {
    tokio::time::pause();

    let handle = tokio::spawn(async {
        from_slice::<u32>(vec![])
            .rbatch(5, Duration::from_millis(100))
            .collect::<Vec<_>>()
            .await
    });

    tokio::time::advance(Duration::from_millis(200)).await;
    let batches = handle.await.unwrap();
    assert!(batches.is_empty());
}

// ── 3. Single item through rmap → rfilter → collect ──────────────────────

#[tokio::test]
async fn test_single_item_pipeline() {
    let results: Vec<AppResult<u32>> = from_slice(vec![42u32])
        .rmap(|x| async move { Ok(x + 1) })
        .rfilter(|r| r.as_ref().map_or(false, |v| *v > 0))
        .collect::<Vec<_>>()
        .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap(), &43u32);
}

// ── 4. Large stream 10k items through rmap ───────────────────────────────

#[tokio::test]
async fn test_large_stream_10k_rmap() {
    let items: Vec<u32> = (0..10_000).collect();
    let results: Vec<AppResult<u32>> = from_slice(items)
        .rmap(|x| async move { Ok(x * 2) })
        .collect::<Vec<_>>()
        .await;

    assert_eq!(results.len(), 10_000);
    for (i, r) in results.iter().enumerate() {
        assert_eq!(*r.as_ref().unwrap(), (i as u32) * 2);
    }
}

// ── 5. Large stream 10k items through rparallel ──────────────────────────

#[tokio::test]
async fn test_large_stream_rparallel() {
    let items: Vec<u32> = (0..10_000).collect();
    let mut results: Vec<u32> = from_slice(items)
        .rparallel(8, |x| async move { Ok::<u32, AppError>(x * 3) })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    results.sort();
    let expected: Vec<u32> = (0..10_000).map(|x| x * 3).collect();
    assert_eq!(results, expected);
}

// ── 6. Error recovery: rmap continues after errors ───────────────────────

#[tokio::test]
async fn test_error_recovery_rmap_continues() {
    let results: Vec<AppResult<u32>> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rmap(|x| async move {
            if x % 2 == 0 {
                Err(AppError::new(ErrorCode::Internal, "even number"))
            } else {
                Ok(x * 10)
            }
        })
        .collect::<Vec<_>>()
        .await;

    assert_eq!(results.len(), 5);
    assert_eq!(*results[0].as_ref().unwrap(), 10);
    assert!(results[1].is_err());
    assert_eq!(*results[2].as_ref().unwrap(), 30);
    assert!(results[3].is_err());
    assert_eq!(*results[4].as_ref().unwrap(), 50);
}

// ── 7. Errors in rparallel ───────────────────────────────────────────────

#[tokio::test]
async fn test_error_in_rparallel() {
    let results: Vec<AppResult<u32>> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rparallel(4, |x| async move {
            if x == 3 || x == 5 {
                Err(AppError::new(ErrorCode::Internal, "bad value"))
            } else {
                Ok(x)
            }
        })
        .collect::<Vec<_>>()
        .await;

    assert_eq!(results.len(), 5);
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count, 3);
    assert_eq!(err_count, 2);
}

// ── 8. rfan_out with one function erroring ───────────────────────────────

#[tokio::test]
async fn test_rfan_out_with_errors() {
    // All closures must have the same concrete type, so use a single closure
    // that conditionally errors based on a captured flag.
    // Instead, use non-capturing closures that error based on input.
    let ok_fn = |x: u32| std::future::ready(Ok::<u32, AppError>(x + 1));
    let err_fn =
        |_x: u32| std::future::ready(Err::<u32, AppError>(AppError::new(ErrorCode::Internal, "fan_out error")));

    let results: Vec<Vec<AppResult<u32>>> = from_slice(vec![10u32, 20])
        .rfan_out(vec![ok_fn, err_fn])
        .collect::<Vec<_>>()
        .await;

    assert_eq!(results.len(), 2);
    // For item 10: first fn succeeds (11), second fn errors
    assert_eq!(*results[0][0].as_ref().unwrap(), 11);
    assert!(results[0][1].is_err());
    // For item 20: first fn succeeds (21), second fn errors
    assert_eq!(*results[1][0].as_ref().unwrap(), 21);
    assert!(results[1][1].is_err());
}

// ── 9. Complex chain: 5 operators ────────────────────────────────────────

#[tokio::test]
async fn test_complex_chain_five_operators() {
    let seen = Arc::new(Mutex::new(Vec::<u32>::new()));
    let seen_clone = seen.clone();

    // from_slice → rmap → rfilter → rtap → rreduce
    let sum = from_slice(vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10])
        .rmap(|x| async move { Ok::<u32, AppError>(x * 2) })
        .rfilter(|r| r.as_ref().map_or(false, |v| *v > 6))
        .rtap(move |r| {
            let seen = seen_clone.clone();
            let val = r.as_ref().ok().copied();
            async move {
                if let Some(v) = val {
                    seen.lock().unwrap().push(v);
                }
            }
        })
        .rreduce(0u32, |acc, r| acc + r.unwrap_or(0))
        .await;

    // Values after rmap: 2,4,6,8,10,12,14,16,18,20
    // After rfilter (>6): 8,10,12,14,16,18,20
    // Sum: 8+10+12+14+16+18+20 = 98
    assert_eq!(sum, 98);
    let tapped = seen.lock().unwrap().clone();
    assert_eq!(tapped, vec![8, 10, 12, 14, 16, 18, 20]);
}

// ── 10. rmap then rbatch ─────────────────────────────────────────────────

#[tokio::test]
async fn test_rmap_then_rbatch() {
    tokio::time::pause();

    let handle = tokio::spawn(async {
        from_slice(vec![1u32, 2, 3, 4, 5])
            .rmap(|x| async move { Ok::<u32, AppError>(x + 100) })
            .rbatch(2, Duration::from_secs(10))
            .collect::<Vec<_>>()
            .await
    });

    tokio::time::advance(Duration::from_secs(11)).await;
    let batches = handle.await.unwrap();

    // 5 items in batches of 2 → [2, 2, 1]
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].len(), 2);
    assert_eq!(batches[1].len(), 2);
    assert_eq!(batches[2].len(), 1);

    // Verify transformed values
    let all: Vec<u32> = batches
        .into_iter()
        .flatten()
        .map(|r: AppResult<u32>| r.unwrap())
        .collect();
    assert_eq!(all, vec![101, 102, 103, 104, 105]);
}

// ── 11. Merge multiple streams ───────────────────────────────────────────

#[tokio::test]
async fn test_merge_multiple_streams() {
    let s1 = from_slice(vec![1u32, 3, 5]);
    let s2 = from_slice(vec![2u32, 4, 6]);

    let mut combined: Vec<u32> = merge(s1, s2).collect::<Vec<_>>().await;
    combined.sort();
    assert_eq!(combined, vec![1, 2, 3, 4, 5, 6]);
}

// ── 12. Concat preserves order ───────────────────────────────────────────

#[tokio::test]
async fn test_concat_preserves_order() {
    let s1 = from_slice(vec![1u32, 2, 3]);
    let s2 = from_slice(vec![4u32, 5, 6]);
    let s3 = from_slice(vec![7u32, 8, 9]);

    let result: Vec<u32> = concat(vec![s1, s2, s3]).collect::<Vec<_>>().await;
    assert_eq!(result, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

// ── 13. Concat with empty streams ────────────────────────────────────────

#[tokio::test]
async fn test_concat_with_empty() {
    let s1 = from_slice::<u32>(vec![]);
    let s2 = from_slice(vec![10u32, 20]);
    let s3 = from_slice::<u32>(vec![]);
    let s4 = from_slice(vec![30u32]);

    let result: Vec<u32> = concat(vec![s1, s2, s3, s4]).collect::<Vec<_>>().await;
    assert_eq!(result, vec![10, 20, 30]);
}

// ── 14. rbatch exact multiple of batch size ──────────────────────────────

#[tokio::test]
async fn test_rbatch_exact_multiple() {
    tokio::time::pause();

    let handle = tokio::spawn(async {
        from_slice(vec![1u32, 2, 3, 4, 5, 6])
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

// ── 15. rbatch with batch size 1 ─────────────────────────────────────────

#[tokio::test]
async fn test_rbatch_single_item_batches() {
    tokio::time::pause();

    let handle = tokio::spawn(async {
        from_slice(vec![10u32, 20, 30])
            .rbatch(1, Duration::from_millis(500))
            .collect::<Vec<_>>()
            .await
    });

    tokio::time::advance(Duration::from_millis(600)).await;
    let batches = handle.await.unwrap();

    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], vec![10]);
    assert_eq!(batches[1], vec![20]);
    assert_eq!(batches[2], vec![30]);
}

// ── 16. rfilter then rparallel ───────────────────────────────────────────

#[tokio::test]
async fn test_rfilter_then_rparallel() {
    let mut results: Vec<u32> = from_slice(vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10])
        .rfilter(|&x| x % 3 == 0)
        .rparallel(4, |x| async move { Ok::<u32, AppError>(x * 100) })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    results.sort();
    assert_eq!(results, vec![300, 600, 900]);
}

// ── 17. Pipeline composition / reuse ─────────────────────────────────────

#[tokio::test]
async fn test_pipeline_composition_reuse() {
    let data = vec![1u32, 2, 3, 4, 5];

    // Chain A: rmap doubling
    let chain_a: Vec<u32> = from_slice(data.clone())
        .rmap(|x| async move { Ok::<u32, AppError>(x * 2) })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(chain_a, vec![2, 4, 6, 8, 10]);

    // Chain B: rfilter + rreduce on same data
    let chain_b = from_slice(data.clone())
        .rfilter(|&x| x > 2)
        .rreduce(0u32, |acc, x| acc + x)
        .await;
    assert_eq!(chain_b, 3 + 4 + 5); // 12

    // Chain C: rmap + rparallel on same data
    let mut chain_c: Vec<u32> = from_slice(data)
        .rmap(|x| async move { Ok::<u32, AppError>(x + 10) })
        .rparallel(2, |r| async move {
            let val = r?;
            Ok(val * 2)
        })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    chain_c.sort();
    assert_eq!(chain_c, vec![22, 24, 26, 28, 30]);
}

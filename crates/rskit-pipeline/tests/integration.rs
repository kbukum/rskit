use futures_util::StreamExt;
use rskit_pipeline::{RskitStreamExt, from_slice};
use std::time::Duration;

#[tokio::test]
async fn rmap_transforms_items() {
    let out: Vec<_> = from_slice(vec![1u32, 2, 3])
        .rmap(|n| async move { Ok(n * 2) })
        .collect::<Vec<_>>()
        .await;
    let values: Vec<u32> = out.into_iter().map(|r| r.unwrap()).collect();
    assert_eq!(values, vec![2u32, 4, 6]);
}

#[tokio::test]
async fn rfilter_removes_items() {
    let out: Vec<u32> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rfilter(|&n| n % 2 == 0)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(out, vec![2u32, 4]);
}

#[tokio::test]
async fn rbatch_groups_items() {
    let out: Vec<Vec<u32>> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rbatch(2, Duration::from_secs(1))
        .collect::<Vec<_>>()
        .await;
    assert_eq!(out[0], vec![1, 2]);
    assert_eq!(out[1], vec![3, 4]);
    assert_eq!(out[2], vec![5]);
}

#[tokio::test]
async fn rbuffer_zero_clamps_to_one() {
    let out: Vec<u32> = from_slice(vec![1u32, 2, 3]).rbuffer(0).collect().await;
    assert_eq!(out, vec![1, 2, 3]);
}

#[tokio::test]
async fn partition_keeps_open_side_after_other_receiver_drops() {
    let (left, right) = from_slice(vec![1u32, 2, 3, 4, 5, 6]).rpartition(|n| n % 2 == 0);
    drop(left);

    let out: Vec<u32> = right.collect().await;
    assert_eq!(out, vec![1, 3, 5]);
}

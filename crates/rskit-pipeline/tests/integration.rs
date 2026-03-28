use rskit_pipeline::{RskitStreamExt, from_slice};

#[tokio::test]
async fn rmap_transforms_items() {
    let out: Vec<_> = from_slice(vec![1u32, 2, 3])
        .rmap(|n| async move { Ok(n * 2) })
        .collect::<Vec<_>>()
        .await;
    assert_eq!(out, vec![Ok(2u32), Ok(4), Ok(6)]);
}

#[tokio::test]
async fn rfilter_removes_items() {
    let out: Vec<u32> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rfilter(|&n| async move { n % 2 == 0 })
        .collect::<Vec<_>>()
        .await;
    assert_eq!(out, vec![2u32, 4]);
}

#[tokio::test]
async fn rbatch_groups_items() {
    let out: Vec<Vec<u32>> = from_slice(vec![1u32, 2, 3, 4, 5])
        .rbatch(2)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(out[0], vec![1, 2]);
    assert_eq!(out[1], vec![3, 4]);
    assert_eq!(out[2], vec![5]);
}

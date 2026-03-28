use rskit_errors::AppResult;
use rskit_worker::{Event, Handler, Pool, PoolConfig};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct DoubleHandler;

#[async_trait::async_trait]
impl Handler<u32, u32> for DoubleHandler {
    async fn handle(
        &self,
        input: u32,
        _emit: mpsc::Sender<Event<u32>>,
        _cancel: CancellationToken,
    ) -> AppResult<u32> {
        Ok(input * 2)
    }
}

#[tokio::test]
async fn pool_doubles_values() {
    let pool = Pool::new(Arc::new(DoubleHandler), PoolConfig::new("test-pool"));
    let h1 = pool.submit(5).await.unwrap();
    let h2 = pool.submit(10).await.unwrap();
    assert_eq!(h1.result().await.unwrap(), 10);
    assert_eq!(h2.result().await.unwrap(), 20);
}

#[tokio::test]
async fn pool_handles_multiple_concurrent_tasks() {
    let pool = Arc::new(Pool::new(
        Arc::new(DoubleHandler),
        PoolConfig::new("concurrent-pool").with_size(4),
    ));
    let handles: Vec<_> = (1u32..=8)
        .map(|n| {
            let pool = pool.clone();
            async move { pool.submit(n).await.unwrap() }
        })
        .collect();
    // futures::future::join_all would be ideal but keep deps minimal
    for (i, h) in handles.into_iter().enumerate() {
        let handle = h.await;
        let result = handle.result().await.unwrap();
        assert_eq!(result, (i as u32 + 1) * 2);
    }
}

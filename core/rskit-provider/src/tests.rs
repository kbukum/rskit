use rskit_errors::AppError;

use crate::traits::{Provider, RequestResponse, Sink};
use crate::{TowerProvider, request_response_fn, sink_fn};

// ── 1. request_response_fn_executes ──────────────────────────────────────

#[tokio::test]
async fn request_response_fn_executes() {
    let provider = request_response_fn("test", |x: i32| async move { Ok(x * 2) });
    let result = provider.execute(21).await;
    assert_eq!(result.unwrap(), 42);
    assert_eq!(provider.name(), "test");
}

// ── 2. sink_fn_sends_value ────────────────────────────────────────────────

#[tokio::test]
async fn sink_fn_sends_value() {
    let sink = sink_fn("sink", |_x: i32| async move { Ok(()) });
    let result = sink.send(1).await;
    assert!(result.is_ok());
}

// ── 3. tower_provider_executes ────────────────────────────────────────────

#[tokio::test]
async fn tower_provider_executes() {
    let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req * 2) });
    let provider = TowerProvider::new("tp", svc);
    let result = provider.execute(5).await;
    assert_eq!(result.unwrap(), 10);
}

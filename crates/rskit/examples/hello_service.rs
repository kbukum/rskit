//! # Hello Service
//!
//! A minimal end-to-end example showing rskit's lifecycle, error handling,
//! and resilience primitives working together.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_service -p rskit --features full
//! ```

use rskit_bootstrap::{AppBuilder, Component, Health};
use rskit_config::{ConfigLoader, ServiceConfig};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_logging::init_logging_env;
use rskit_resilience::{CbConfig, CircuitBreaker, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// A toy downstream "service" that fails twice before succeeding
// ---------------------------------------------------------------------------
static CALL_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

async fn call_downstream() -> AppResult<String> {
    let n = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if n < 2 {
        tracing::warn!("downstream call failed (attempt {})", n + 1);
        Err(AppError::new(ErrorCode::ServiceUnavailable, "not ready yet"))
    } else {
        tracing::info!("downstream call succeeded on attempt {}", n + 1);
        Ok("pong".to_string())
    }
}

// ---------------------------------------------------------------------------
// A component that calls downstream on start
// ---------------------------------------------------------------------------
struct PingComponent {
    cb: CircuitBreaker,
    retry: RetryPolicy,
}

#[async_trait::async_trait]
impl Component for PingComponent {
    async fn start(&self, _cancel: CancellationToken) -> AppResult<()> {
        let cb = self.cb.clone();
        let retry = self.retry.clone();

        let result = retry
            .execute(|| {
                let cb = cb.clone();
                async move { cb.execute(|| call_downstream()).await }
            })
            .await?;

        tracing::info!("PingComponent got: {}", result);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        tracing::info!("PingComponent stopping");
        Ok(())
    }

    async fn health(&self) -> Health {
        Health::healthy("ping")
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> AppResult<()> {
    let _guard = init_logging_env();

    let cb = CircuitBreaker::new(CbConfig::new("downstream").with_max_failures(5));
    let retry = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(50));

    let component = Arc::new(PingComponent { cb, retry });

    // Normally you'd load this from a file / env
    let _config = ConfigLoader::new().load::<rskit_config::ServiceConfig>().ok();

    AppBuilder::new(())
        .build()?
        .on_start(move |_cfg, cancel| {
            let comp = component.clone();
            async move {
                comp.start(cancel).await?;
                tracing::info!("All components started. Press Ctrl-C to stop.");
                Ok(())
            }
        })
        .run()
        .await
}

//! Agent Demo — Interactive media processing agent built with rskit.
//!
//! Demonstrates: rskit-worker (background tasks with progress),
//! rskit-cli (progress bars, output tables, cancellation),
//! rskit-pipeline (stream processing), rskit-file (I/O),
//! rskit-media + rskit-media-image (actual image processing).
//!
//! Run: cargo run -p agent-demo

mod dashboard;
mod shell;
mod tasks;

use rskit_errors::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    shell::run(cancel).await
}

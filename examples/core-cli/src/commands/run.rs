//! `run` subcommand: a bounded work loop that honours cancellation.

use std::time::Duration;

use rskit_cli::{CancellationToken, OutputKV};
use rskit_logging::info;

/// Process `units` of simulated work, stopping early if `token` is cancelled.
///
/// Demonstrates cooperative cancellation and lifecycle ownership using only
/// core crates — no service runtime required. Each step is bounded and races
/// the cancellation token, so Ctrl+C winds the loop down promptly.
pub async fn execute(units: u32, token: &CancellationToken) -> OutputKV {
    let mut processed = 0u32;
    for unit in 0..units {
        if token.is_cancelled() {
            break;
        }
        tokio::select! {
            () = token.cancelled() => break,
            () = tokio::time::sleep(Duration::from_millis(10)) => {
                processed += 1;
                info!(unit, "processed work unit");
            }
        }
    }

    let mut kv = OutputKV::new();
    kv.add("requested", units.to_string())
        .add("processed", processed.to_string())
        .add("cancelled", token.is_cancelled().to_string());
    kv
}

//! Bench runner — orchestrates the complete benchmark lifecycle.
//!
//! Split by concern: run configuration ([`RunOptions`]), per-sample worker
//! evaluation and its outcome, and the [`BenchRunner`] that drives registered
//! branches to a result.

mod bench_runner;
mod evaluation;
mod options;

pub use bench_runner::BenchRunner;
pub use options::RunOptions;

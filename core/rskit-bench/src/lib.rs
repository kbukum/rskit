//! ML benchmarking framework: evaluators, metrics, reports, visualization.
//!
//! Migrated from sentinel/rs-services/ruskit to use rskit-errors (AppResult/AppError)
//! instead of anyhow, and rskit-provider for the Provider/RequestResponse traits.
//!
//! Benchmark orchestration accepts injected clocks and storage. Use
//! [`FixedClock`] and a test-owned [`RunStorage`] implementation for deterministic
//! tests; use [`SystemClock`] and [`FileRunStorage`] for normal CLI runs.

pub mod cli;
pub mod compare;
pub mod curves;
pub mod dataset;
pub mod dataset_loader;
pub mod evaluator;
pub mod execution;
pub mod metric;
pub mod metrics;
pub mod middleware;
pub mod report;
pub mod report_gen;
pub mod result;
mod run_id;
pub mod run_storage;
pub mod runner;
pub mod schema;
pub mod types;
pub mod viz;

// Primary crate API.
pub use evaluator::{Evaluator, EvaluatorFunc, FromProvider};
pub use execution::BenchExecutionPlan;
pub use result::{BenchRunResult, MetricResult};
pub use rskit_util::time::{
    Clock as BenchClock, FixedClock, SharedClock, SystemClock, system_clock,
};
pub use run_storage::{FileRunStorage, ListOptions, RunStorage};
pub use runner::{BenchRunner, RunOptions};
pub use types::{BenchSample, Prediction, ScoredSample};

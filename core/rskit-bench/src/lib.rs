//! ML benchmarking framework: evaluators, metrics, reports, visualization.
//!
//! Migrated from sentinel/rs-services/ruskit to use rskit-errors (AppResult/AppError)
//! instead of anyhow, and rskit-provider for the Provider/RequestResponse traits.

pub mod cli;
pub mod compare;
pub mod curves;
pub mod dataset;
pub mod dataset_loader;
pub mod evaluator;
pub mod metric;
pub mod metrics;
pub mod middleware;
pub mod report;
pub mod report_gen;
pub mod result;
pub mod run_storage;
pub mod runner;
pub mod schema;
pub mod storage;
pub mod types;
pub mod viz;

// Primary crate API.
pub use evaluator::{Evaluator, EvaluatorFunc, FromProvider};
pub use result::{BenchRunResult, MetricResult};
pub use run_storage::{FileRunStorage, ListOptions, RunStorage};
pub use runner::{BenchRunner, RunOptions};
pub use types::{BenchSample, Prediction, ScoredSample};

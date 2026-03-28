//! ML benchmarking framework: evaluators, metrics, reports, visualization.

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

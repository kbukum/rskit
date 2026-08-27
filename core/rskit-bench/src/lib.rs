//! ML benchmarking framework: evaluators, metrics, reports, visualization.
//!
//! Benchmark orchestration accepts injected clocks, storage, and a provenance
//! probe. Use [`FixedClock`], a test-owned [`RunStorage`] implementation, and a
//! [`FixedProvenanceProbe`] for deterministic, reproducible tests; use
//! [`SystemClock`], [`FileRunStorage`], and the default [`SystemProvenanceProbe`]
//! for normal CLI runs. Every [`BenchRunResult`] carries a [`RunProvenance`] record
//! (seed, source commit, tool/host identity, order-independent dataset hash) so a
//! run can be reproduced and audited; set the seed with
//! [`RunOptions::with_seed`](crate::RunOptions::with_seed).

#![warn(missing_docs)]

pub mod cli;
pub mod compare;
pub mod curves;
pub mod dataset;
pub mod dataset_loader;
pub mod evaluator;
pub mod execution;
pub mod metric;
pub mod middleware;
pub mod provenance;
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
pub use provenance::{FixedProvenanceProbe, ProvenanceProbe, RunProvenance, SystemProvenanceProbe};
pub use result::{BenchRunResult, MetricResult};
pub use rskit_util::time::{
    Clock as BenchClock, FixedClock, SharedClock, SystemClock, system_clock,
};
pub use run_storage::{FileRunStorage, ListOptions, RunStorage};
pub use runner::{BenchRunner, RunOptions};
pub use types::{BenchSample, Prediction, ScoredSample};

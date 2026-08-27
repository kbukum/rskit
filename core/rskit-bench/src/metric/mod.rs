//! Pluggable evaluation metrics for the bench framework.

mod adapter;
mod classification;
mod composite;
mod matching;
mod probability;
mod ranking;
mod regression;
mod tokens;

pub use adapter::{RunMetric, as_run_metric, as_run_metrics};
pub use classification::{
    binary_classification, confusion_matrix, multi_class_classification, threshold_sweep,
};
pub use composite::weighted;
pub use matching::{exact_match, fuzzy_match};
pub use probability::{auc_roc, brier_score, calibration, log_loss};
pub use ranking::{mean_average_precision, ndcg, precision_at_k, recall_at_k};
pub use regression::{mae, mse, r_squared, rmse};
pub use tokens::token_stats;

mod suite;

pub use suite::{Metric, Suite};

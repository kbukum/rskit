//! Render-all orchestrator and configuration.

use crate::curves::*;
use crate::result::{BenchRunResult, BenchSampleResult, MetricResult};
use std::collections::HashMap;

/// Configuration option for chart rendering.
pub struct RenderOption(Box<dyn Fn(&mut RenderConfig)>);

struct RenderConfig {
    width: usize,
    height: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 600,
            height: 400,
        }
    }
}

/// Create a render option that sets chart dimensions.
pub fn with_size(w: usize, h: usize) -> RenderOption {
    RenderOption(Box::new(move |cfg| {
        cfg.width = w;
        cfg.height = h;
    }))
}

/// Render all available charts from a benchmark result.
///
/// Returns a map of filename → SVG content.
/// Only charts with sufficient data are included.
pub fn render_all(result: &BenchRunResult, opts: &[RenderOption]) -> HashMap<String, String> {
    let mut cfg = RenderConfig::default();
    for opt in opts {
        (opt.0)(&mut cfg);
    }

    let mut out = HashMap::new();

    // Confusion matrix
    if let Some(cm) = extract_curve::<ConfusionMatrixDetail>(&result.curves, "confusion_matrix")
        .or_else(|| extract_from_metrics::<ConfusionMatrixDetail>(&result.metrics))
    {
        let s = super::render_confusion(&cm, cfg.width, cfg.height);
        if !s.is_empty() {
            out.insert("confusion_matrix.svg".to_string(), s);
        }
    }

    // ROC curve
    if let Some(roc) = extract_curve::<RocCurve>(&result.curves, "roc")
        .or_else(|| extract_from_metrics::<RocCurve>(&result.metrics))
    {
        out.insert(
            "roc_curve.svg".to_string(),
            super::render_roc(&roc, cfg.width, cfg.height),
        );
    }

    // Calibration curve
    if let Some(cal) = extract_curve::<CalibrationCurve>(&result.curves, "calibration")
        .or_else(|| extract_from_metrics::<CalibrationCurve>(&result.metrics))
    {
        out.insert(
            "calibration_curve.svg".to_string(),
            super::render_calibration(&cal, cfg.width, cfg.height),
        );
    }

    // Score distribution
    let dists = extract_curve::<Vec<ScoreDistribution>>(&result.curves, "score_distribution")
        .or_else(|| extract_from_metrics::<Vec<ScoreDistribution>>(&result.metrics))
        .unwrap_or_else(|| build_distributions(&result.samples));
    if !dists.is_empty() {
        let s = super::render_distribution(&dists, cfg.width, cfg.height);
        if !s.is_empty() {
            out.insert("score_distribution.svg".to_string(), s);
        }
    }

    // Branch comparison
    if !result.branches.is_empty() {
        let s = super::render_comparison(&result.branches, cfg.width, cfg.height);
        if !s.is_empty() {
            out.insert("branch_comparison.svg".to_string(), s);
        }
    }

    out
}

fn decode_as<T: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Option<T> {
    serde_json::from_value(v.clone()).ok()
}

fn extract_curve<T: serde::de::DeserializeOwned>(
    curves: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<T> {
    curves.get(key).and_then(|v| decode_as(v))
}

fn extract_from_metrics<T: serde::de::DeserializeOwned>(metrics: &[MetricResult]) -> Option<T> {
    for m in metrics {
        if let Some(ref detail) = m.detail
            && let Some(t) = decode_as::<T>(detail)
        {
            return Some(t);
        }
    }
    None
}

/// Build 10-bin score distributions from raw samples, grouped by label.
fn build_distributions(samples: &[BenchSampleResult]) -> Vec<ScoreDistribution> {
    if samples.is_empty() {
        return Vec::new();
    }

    let mut by_label: HashMap<String, Vec<f64>> = HashMap::new();
    for s in samples {
        by_label.entry(s.label.clone()).or_default().push(s.score);
    }

    let bins: Vec<f64> = (0..=10).map(|i| i as f64 / 10.0).collect();

    let mut labels: Vec<String> = by_label.keys().cloned().collect();
    labels.sort();

    labels
        .into_iter()
        .map(|label| {
            let scores = &by_label[&label];
            let mut counts = vec![0usize; 10];
            for &s in scores {
                let idx = ((s * 10.0).floor() as usize).min(9);
                counts[idx] += 1;
            }
            ScoreDistribution {
                label,
                bins: bins.clone(),
                counts,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{BenchRunResult, DatasetInfo};

    fn make_result() -> BenchRunResult {
        BenchRunResult {
            id: "test".to_string(),
            schema: String::new(),
            version: String::new(),
            timestamp: String::new(),
            tag: String::new(),
            duration_ms: 0,
            dataset: DatasetInfo {
                name: "test".to_string(),
                version: String::new(),
                sample_count: 0,
                label_distribution: HashMap::new(),
            },
            metrics: Vec::new(),
            branches: HashMap::new(),
            samples: Vec::new(),
            curves: HashMap::new(),
        }
    }

    #[test]
    fn test_render_all_empty() {
        let r = make_result();
        let charts = render_all(&r, &[]);
        assert!(charts.is_empty());
    }

    #[test]
    fn test_render_all_with_roc() {
        let mut r = make_result();
        let roc = RocCurve {
            fpr: vec![0.0, 0.1, 0.5, 1.0],
            tpr: vec![0.0, 0.4, 0.8, 1.0],
            thresholds: vec![1.0, 0.8, 0.5, 0.0],
            auc: 0.85,
        };
        r.curves
            .insert("roc".to_string(), serde_json::to_value(&roc).unwrap());
        let charts = render_all(&r, &[]);
        assert!(charts.contains_key("roc_curve.svg"));
        assert!(charts["roc_curve.svg"].contains("<svg"));
        assert!(charts["roc_curve.svg"].contains("ROC Curve"));
    }

    #[test]
    fn test_render_all_with_size() {
        let mut r = make_result();
        let roc = RocCurve {
            fpr: vec![0.0, 1.0],
            tpr: vec![0.0, 1.0],
            thresholds: vec![1.0, 0.0],
            auc: 0.5,
        };
        r.curves
            .insert("roc".to_string(), serde_json::to_value(&roc).unwrap());
        let charts = render_all(&r, &[with_size(800, 600)]);
        assert!(charts["roc_curve.svg"].contains(r#"width="800""#));
        assert!(charts["roc_curve.svg"].contains(r#"height="600""#));
    }

    #[test]
    fn test_build_distributions() {
        let samples = vec![
            BenchSampleResult {
                id: "1".into(),
                label: "positive".into(),
                predicted: "positive".into(),
                score: 0.9,
                correct: true,
                branch_scores: HashMap::new(),
                duration_ms: 0,
                error: String::new(),
            },
            BenchSampleResult {
                id: "2".into(),
                label: "negative".into(),
                predicted: "negative".into(),
                score: 0.2,
                correct: true,
                branch_scores: HashMap::new(),
                duration_ms: 0,
                error: String::new(),
            },
        ];
        let dists = build_distributions(&samples);
        assert_eq!(dists.len(), 2);
        assert_eq!(dists[0].label, "negative");
        assert_eq!(dists[1].label, "positive");
    }

    #[test]
    fn test_render_all_builds_distributions_from_samples() {
        let mut r = make_result();
        r.samples = vec![
            BenchSampleResult {
                id: "1".into(),
                label: "pos".into(),
                predicted: "pos".into(),
                score: 0.8,
                correct: true,
                branch_scores: HashMap::new(),
                duration_ms: 0,
                error: String::new(),
            },
            BenchSampleResult {
                id: "2".into(),
                label: "neg".into(),
                predicted: "neg".into(),
                score: 0.3,
                correct: true,
                branch_scores: HashMap::new(),
                duration_ms: 0,
                error: String::new(),
            },
        ];
        let charts = render_all(&r, &[]);
        assert!(charts.contains_key("score_distribution.svg"));
    }
}

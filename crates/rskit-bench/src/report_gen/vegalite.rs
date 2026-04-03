//! Vega-Lite spec generation for bench visualizations.

use super::{Reporter, io_err};
use crate::result::BenchRunResult;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::io::Write;

/// Generates a Vega-Lite JSON spec document from bench results.
pub struct VegaLiteReporter;

impl Reporter for VegaLiteReporter {
    fn name(&self) -> &str {
        "vegalite"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let specs = vegalite_specs(result);
        let json = serde_json::to_string_pretty(&specs)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("serialize: {e}")))?;
        write!(w, "{json}").map_err(io_err)?;
        Ok(())
    }
}

/// Generate all available Vega-Lite spec objects for the given run result.
pub fn vegalite_specs(result: &BenchRunResult) -> serde_json::Value {
    let mut specs = serde_json::Map::new();

    // ROC curve spec (if roc data is present in curves)
    if let Some(roc_data) = result.curves.get("roc") {
        if let Some(spec) = roc_spec(roc_data) {
            specs.insert("roc".to_string(), spec);
        }
    }

    // Confusion matrix heatmap
    for m in &result.metrics {
        if let Some(ref detail) = m.detail {
            if detail.get("matrix").is_some() && detail.get("labels").is_some() {
                if let Some(spec) = confusion_matrix_spec(detail) {
                    specs.insert("confusion_matrix".to_string(), spec);
                }
            }
        }
    }

    // Score distribution
    if let Some(dist_data) = result.curves.get("score_distribution") {
        if let Some(spec) = score_distribution_spec(dist_data) {
            specs.insert("score_distribution".to_string(), spec);
        }
    }

    // Threshold sweep
    if let Some(sweep_data) = result.curves.get("threshold_sweep") {
        if let Some(spec) = threshold_sweep_spec(sweep_data) {
            specs.insert("threshold_sweep".to_string(), spec);
        }
    }

    // Calibration curve
    if let Some(cal_data) = result.curves.get("calibration") {
        if let Some(spec) = calibration_spec(cal_data) {
            specs.insert("calibration".to_string(), spec);
        }
    }

    // Branch comparison
    if !result.branches.is_empty() {
        specs.insert(
            "branch_comparison".to_string(),
            branch_comparison_spec(result),
        );
    }

    serde_json::Value::Object(specs)
}

fn roc_spec(roc_data: &serde_json::Value) -> Option<serde_json::Value> {
    let fpr = roc_data.get("fpr")?.as_array()?;
    let tpr = roc_data.get("tpr")?.as_array()?;
    let auc = roc_data.get("auc").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let mut values = Vec::new();
    for (f, t) in fpr.iter().zip(tpr.iter()) {
        values.push(serde_json::json!({
            "fpr": f,
            "tpr": t
        }));
    }

    Some(serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": format!("ROC Curve (AUC={:.4})", auc),
        "width": 400,
        "height": 400,
        "layer": [
            {
                "data": { "values": values },
                "mark": { "type": "line", "color": "#1f77b4" },
                "encoding": {
                    "x": { "field": "fpr", "type": "quantitative", "title": "False Positive Rate" },
                    "y": { "field": "tpr", "type": "quantitative", "title": "True Positive Rate" }
                }
            },
            {
                "data": { "values": [{"fpr": 0, "tpr": 0}, {"fpr": 1, "tpr": 1}] },
                "mark": { "type": "line", "strokeDash": [4, 4], "color": "gray" },
                "encoding": {
                    "x": { "field": "fpr", "type": "quantitative" },
                    "y": { "field": "tpr", "type": "quantitative" }
                }
            }
        ]
    }))
}

fn confusion_matrix_spec(detail: &serde_json::Value) -> Option<serde_json::Value> {
    let labels = detail.get("labels")?.as_array()?;
    let matrix = detail.get("matrix")?.as_array()?;

    let mut values = Vec::new();
    for (i, row) in matrix.iter().enumerate() {
        if let Some(cells) = row.as_array() {
            for (j, cell) in cells.iter().enumerate() {
                let actual = labels.get(i).and_then(|v| v.as_str()).unwrap_or("?");
                let predicted = labels.get(j).and_then(|v| v.as_str()).unwrap_or("?");
                values.push(serde_json::json!({
                    "actual": actual,
                    "predicted": predicted,
                    "count": cell
                }));
            }
        }
    }

    Some(serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": "Confusion Matrix",
        "width": 300,
        "height": 300,
        "data": { "values": values },
        "mark": "rect",
        "encoding": {
            "x": { "field": "predicted", "type": "nominal", "title": "Predicted" },
            "y": { "field": "actual", "type": "nominal", "title": "Actual" },
            "color": { "field": "count", "type": "quantitative", "title": "Count" }
        }
    }))
}

fn score_distribution_spec(dist_data: &serde_json::Value) -> Option<serde_json::Value> {
    let distributions = dist_data.as_array()?;
    let mut values = Vec::new();
    for dist in distributions {
        let label = dist.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let bins = dist.get("bins").and_then(|v| v.as_array())?;
        let counts = dist.get("counts").and_then(|v| v.as_array())?;
        for (b, c) in bins.iter().zip(counts.iter()) {
            values.push(serde_json::json!({
                "label": label,
                "score": b,
                "count": c
            }));
        }
    }

    Some(serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": "Score Distribution",
        "width": 400,
        "height": 300,
        "data": { "values": values },
        "mark": "bar",
        "encoding": {
            "x": { "field": "score", "type": "quantitative", "title": "Score", "bin": true },
            "y": { "field": "count", "type": "quantitative", "title": "Count" },
            "color": { "field": "label", "type": "nominal" }
        }
    }))
}

fn threshold_sweep_spec(sweep_data: &serde_json::Value) -> Option<serde_json::Value> {
    let points = sweep_data.as_array()?;
    let mut values = Vec::new();
    for pt in points {
        let threshold = pt.get("threshold").and_then(|v| v.as_f64())?;
        let precision = pt.get("precision").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let recall = pt.get("recall").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let f1 = pt.get("f1").and_then(|v| v.as_f64()).unwrap_or(0.0);
        values.push(
            serde_json::json!({"threshold": threshold, "metric": "precision", "value": precision}),
        );
        values
            .push(serde_json::json!({"threshold": threshold, "metric": "recall", "value": recall}));
        values.push(serde_json::json!({"threshold": threshold, "metric": "f1", "value": f1}));
    }

    Some(serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": "Threshold Sweep",
        "width": 400,
        "height": 300,
        "data": { "values": values },
        "mark": "line",
        "encoding": {
            "x": { "field": "threshold", "type": "quantitative", "title": "Threshold" },
            "y": { "field": "value", "type": "quantitative", "title": "Value" },
            "color": { "field": "metric", "type": "nominal" }
        }
    }))
}

fn calibration_spec(cal_data: &serde_json::Value) -> Option<serde_json::Value> {
    let predicted = cal_data
        .get("predicted_probability")
        .and_then(|v| v.as_array())?;
    let actual = cal_data
        .get("actual_frequency")
        .and_then(|v| v.as_array())?;

    let mut values = Vec::new();
    for (p, a) in predicted.iter().zip(actual.iter()) {
        values.push(serde_json::json!({
            "predicted": p,
            "actual": a
        }));
    }

    Some(serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": "Calibration Curve",
        "width": 400,
        "height": 400,
        "layer": [
            {
                "data": { "values": values },
                "mark": { "type": "point", "color": "#1f77b4" },
                "encoding": {
                    "x": { "field": "predicted", "type": "quantitative", "title": "Predicted Probability" },
                    "y": { "field": "actual", "type": "quantitative", "title": "Actual Frequency" }
                }
            },
            {
                "data": { "values": [{"x": 0, "y": 0}, {"x": 1, "y": 1}] },
                "mark": { "type": "line", "strokeDash": [4, 4], "color": "gray" },
                "encoding": {
                    "x": { "field": "x", "type": "quantitative" },
                    "y": { "field": "y", "type": "quantitative" }
                }
            }
        ]
    }))
}

fn branch_comparison_spec(result: &BenchRunResult) -> serde_json::Value {
    let mut values = Vec::new();
    for (name, br) in &result.branches {
        values.push(serde_json::json!({
            "branch": name,
            "metric": "avg_score_positive",
            "value": br.avg_score_positive
        }));
        values.push(serde_json::json!({
            "branch": name,
            "metric": "avg_score_negative",
            "value": br.avg_score_negative
        }));
        for (mk, mv) in &br.metrics {
            values.push(serde_json::json!({
                "branch": name,
                "metric": mk,
                "value": mv
            }));
        }
    }

    serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "title": "Branch Comparison",
        "width": 400,
        "height": 300,
        "data": { "values": values },
        "mark": "bar",
        "encoding": {
            "x": { "field": "branch", "type": "nominal", "title": "Branch" },
            "y": { "field": "value", "type": "quantitative", "title": "Value" },
            "color": { "field": "metric", "type": "nominal" },
            "xOffset": { "field": "metric" }
        }
    })
}

//! ROC curve SVG rendering.

use super::svg::{Point, Svg, clamp01, draw_axes};
use crate::curves::RocCurve;

const PAD_LEFT: usize = 60;
const PAD_TOP: usize = 40;
const PAD_RIGHT: usize = 20;
const PAD_BOTTOM: usize = 50;

/// Render a ROC curve as a standalone SVG string.
pub fn render_roc(roc: &RocCurve, width: usize, height: usize) -> String {
    let plot_w = (width - PAD_LEFT - PAD_RIGHT) as f64;
    let plot_h = (height - PAD_TOP - PAD_BOTTOM) as f64;
    let left = PAD_LEFT as f64;
    let top = PAD_TOP as f64;

    let mut svg = Svg::new(width, height);

    // Title
    svg.text(
        width as f64 / 2.0,
        24.0,
        "ROC Curve",
        "#333",
        14,
        r#"text-anchor="middle" font-weight="bold""#,
    );

    // Axes
    draw_axes(&mut svg, PAD_LEFT, PAD_TOP, plot_w, plot_h);

    // Grid lines and tick labels (5 divisions: 0.0, 0.25, 0.5, 0.75, 1.0)
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let x = left + frac * plot_w;
        let y = top + plot_h - frac * plot_h;

        svg.line(x, top, x, top + plot_h, "#eee", 1.0, "");
        svg.line(left, y, left + plot_w, y, "#eee", 1.0, "");

        svg.text(
            x,
            top + plot_h + 16.0,
            &format!("{frac:.2}"),
            "#666",
            10,
            r#"text-anchor="middle""#,
        );
        svg.text(
            left - 8.0,
            y + 4.0,
            &format!("{frac:.2}"),
            "#666",
            10,
            r#"text-anchor="end""#,
        );
    }

    // Diagonal reference line (random classifier)
    svg.line(
        left,
        top + plot_h,
        left + plot_w,
        top,
        "#999",
        1.0,
        r#"stroke-dasharray="5,5""#,
    );

    // ROC curve polyline
    let points: Vec<Point> = roc
        .fpr
        .iter()
        .zip(roc.tpr.iter())
        .map(|(&fpr, &tpr)| Point {
            x: left + clamp01(fpr) * plot_w,
            y: top + plot_h - clamp01(tpr) * plot_h,
        })
        .collect();
    svg.polyline(&points, "#4285F4", 2.0, "none", "");

    // AUC annotation
    svg.text(
        left + plot_w - 10.0,
        top + plot_h - 10.0,
        &format!("AUC = {:.4}", roc.auc),
        "#333",
        11,
        r#"text-anchor="end" font-weight="bold""#,
    );

    // Axis labels
    svg.text(
        left + plot_w / 2.0,
        (height - 6) as f64,
        "False Positive Rate",
        "#333",
        11,
        r#"text-anchor="middle""#,
    );
    svg.text(
        14.0,
        top + plot_h / 2.0,
        "True Positive Rate",
        "#333",
        11,
        &format!(
            r#"text-anchor="middle" transform="rotate(-90, 14, {:.1})""#,
            top + plot_h / 2.0
        ),
    );

    svg.render()
}

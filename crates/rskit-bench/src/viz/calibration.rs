//! Calibration curve SVG rendering.

use super::svg::{Point, Svg, clamp01, draw_axes};
use crate::curves::CalibrationCurve;

const PAD_LEFT: usize = 60;
const PAD_TOP: usize = 40;
const PAD_RIGHT: usize = 20;
const PAD_BOTTOM: usize = 50;

/// Render a calibration curve as a standalone SVG string.
pub fn render_calibration(cal: &CalibrationCurve, width: usize, height: usize) -> String {
    let plot_w = (width - PAD_LEFT - PAD_RIGHT) as f64;
    let plot_h = (height - PAD_TOP - PAD_BOTTOM) as f64;
    let left = PAD_LEFT as f64;
    let top = PAD_TOP as f64;

    let mut svg = Svg::new(width, height);

    // Title
    svg.text(
        width as f64 / 2.0,
        24.0,
        "Calibration Curve",
        "#333",
        14,
        r#"text-anchor="middle" font-weight="bold""#,
    );

    // Axes
    draw_axes(&mut svg, PAD_LEFT, PAD_TOP, plot_w, plot_h);

    // Grid and tick labels (5 divisions)
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let x = left + frac * plot_w;
        let y = top + plot_h - frac * plot_h;

        svg.line(x, top, x, top + plot_h, "#eee", 1.0, "");
        svg.line(left, y, left + plot_w, y, "#eee", 1.0, "");

        svg.text(
            x,
            top + plot_h + 16.0,
            &format!("{:.2}", frac),
            "#666",
            10,
            r#"text-anchor="middle""#,
        );
        svg.text(
            left - 8.0,
            y + 4.0,
            &format!("{:.2}", frac),
            "#666",
            10,
            r#"text-anchor="end""#,
        );
    }

    // Diagonal reference line (perfect calibration)
    svg.line(
        left,
        top + plot_h,
        left + plot_w,
        top,
        "#999",
        1.0,
        r#"stroke-dasharray="5,5""#,
    );

    // Calibration curve polyline
    let points: Vec<Point> = cal
        .predicted_probability
        .iter()
        .zip(cal.actual_frequency.iter())
        .map(|(&pp, &af)| Point {
            x: left + clamp01(pp) * plot_w,
            y: top + plot_h - clamp01(af) * plot_h,
        })
        .collect();
    svg.polyline(&points, "#EA4335", 2.0, "none", "");

    // Data point circles
    for p in &points {
        svg.circle(p.x, p.y, 3.0, "#EA4335", "");
    }

    // Axis labels
    svg.text(
        left + plot_w / 2.0,
        (height - 6) as f64,
        "Predicted Probability",
        "#333",
        11,
        r#"text-anchor="middle""#,
    );
    svg.text(
        14.0,
        top + plot_h / 2.0,
        "Actual Frequency",
        "#333",
        11,
        &format!(
            r#"text-anchor="middle" transform="rotate(-90, 14, {:.1})""#,
            top + plot_h / 2.0
        ),
    );

    svg.render()
}

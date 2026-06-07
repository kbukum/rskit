//! Confusion matrix heatmap SVG rendering.

use super::svg::{Svg, heat_color};
use crate::curves::ConfusionMatrixDetail;

const PAD_LEFT: usize = 90;
const PAD_TOP: usize = 60;
const PAD_RIGHT: usize = 20;
const PAD_BOTTOM: usize = 60;

/// Render a confusion matrix heatmap as a standalone SVG string.
pub fn render_confusion(cm: &ConfusionMatrixDetail, width: usize, height: usize) -> String {
    let n = cm.labels.len();
    if n == 0 {
        return String::new();
    }

    let cell_w = (width - PAD_LEFT - PAD_RIGHT) as f64 / n as f64;
    let cell_h = (height - PAD_TOP - PAD_BOTTOM) as f64 / n as f64;
    let left = PAD_LEFT as f64;
    let top = PAD_TOP as f64;

    let mut svg = Svg::new(width, height);

    // Title
    svg.text(
        width as f64 / 2.0,
        24.0,
        "Confusion Matrix",
        "#333",
        14,
        r#"text-anchor="middle" font-weight="bold""#,
    );

    // Find max value for color scaling
    let max_val = cm
        .matrix
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    // Render cells
    for (row, row_data) in cm.matrix.iter().enumerate() {
        for (col, &val) in row_data.iter().enumerate() {
            let intensity = val as f64 / max_val;
            let fill = heat_color(intensity);
            let x = left + col as f64 * cell_w;
            let y = top + row as f64 * cell_h;

            svg.rect_f(
                x,
                y,
                cell_w,
                cell_h,
                &fill,
                r#"stroke="white" stroke-width="2""#,
            );

            let text_color = if intensity <= 0.5 { "#333" } else { "white" };
            svg.text(
                x + cell_w / 2.0,
                y + cell_h / 2.0 + 5.0,
                &val.to_string(),
                text_color,
                12,
                r#"text-anchor="middle""#,
            );
        }
    }

    // Row labels (actual)
    for (i, label) in cm.labels.iter().enumerate() {
        svg.text(
            left - 8.0,
            top + i as f64 * cell_h + cell_h / 2.0 + 5.0,
            label,
            "#333",
            10,
            r#"text-anchor="end""#,
        );
    }

    // Column labels (predicted)
    for (i, label) in cm.labels.iter().enumerate() {
        svg.text(
            left + i as f64 * cell_w + cell_w / 2.0,
            top - 8.0,
            label,
            "#333",
            10,
            r#"text-anchor="middle""#,
        );
    }

    // Axis labels
    svg.text(
        left + (n as f64 * cell_w) / 2.0,
        (height - 10) as f64,
        "Predicted",
        "#333",
        11,
        r#"text-anchor="middle""#,
    );
    svg.text(
        14.0,
        top + (n as f64 * cell_h) / 2.0,
        "Actual",
        "#333",
        11,
        &format!(
            r#"text-anchor="middle" transform="rotate(-90, 14, {:.1})""#,
            top + (n as f64 * cell_h) / 2.0
        ),
    );

    svg.render()
}

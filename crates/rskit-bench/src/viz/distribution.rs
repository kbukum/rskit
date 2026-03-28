//! Score distribution histogram SVG rendering.

use super::svg::{Svg, color_at, draw_axes};
use crate::curves::ScoreDistribution;

const PAD_LEFT: usize = 60;
const PAD_TOP: usize = 50;
const PAD_RIGHT: usize = 20;
const PAD_BOTTOM: usize = 50;

/// Render score distributions as a grouped histogram SVG string.
pub fn render_distribution(dists: &[ScoreDistribution], width: usize, height: usize) -> String {
    if dists.is_empty() {
        return String::new();
    }

    let plot_w = (width - PAD_LEFT - PAD_RIGHT) as f64;
    let plot_h = (height - PAD_TOP - PAD_BOTTOM) as f64;
    let left = PAD_LEFT as f64;
    let top = PAD_TOP as f64;

    let mut svg = Svg::new(width, height);

    // Title
    svg.text(
        width as f64 / 2.0,
        24.0,
        "Score Distribution",
        "#333",
        14,
        r#"text-anchor="middle" font-weight="bold""#,
    );

    // Axes
    draw_axes(&mut svg, PAD_LEFT, PAD_TOP, plot_w, plot_h);

    // Find max count across all distributions for Y scaling
    let max_count = dists
        .iter()
        .flat_map(|d| d.counts.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    // Y grid: 4 divisions
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let y = top + plot_h - frac * plot_h;
        svg.line(left, y, left + plot_w, y, "#eee", 1.0, "");
        let label = (frac * max_count) as usize;
        svg.text(
            left - 8.0,
            y + 4.0,
            &label.to_string(),
            "#666",
            10,
            r#"text-anchor="end""#,
        );
    }

    // X labels: 0.0 to 1.0 in 0.1 increments
    for i in 0..=10 {
        let frac = i as f64 / 10.0;
        let x = left + frac * plot_w;
        svg.text(
            x,
            top + plot_h + 16.0,
            &format!("{:.1}", frac),
            "#666",
            10,
            r#"text-anchor="middle""#,
        );
    }

    // Determine number of bins from first distribution
    let n_bins = dists[0].counts.len();
    if n_bins == 0 {
        return svg.render();
    }
    let n_labels = dists.len();
    let bin_width = plot_w / n_bins as f64;
    let bar_width = bin_width / (n_labels as f64 + 1.0);

    // Grouped bars
    for (li, dist) in dists.iter().enumerate() {
        let color = color_at(li);
        for (bi, &count) in dist.counts.iter().enumerate() {
            let bar_h = (count as f64 / max_count) * plot_h;
            let x = left + bi as f64 * bin_width + li as f64 * bar_width + bar_width * 0.15;
            let y = top + plot_h - bar_h;
            svg.rect_f(x, y, bar_width * 0.7, bar_h, color, r#"opacity="0.75""#);
        }
    }

    // Legend: upper-left
    for (i, dist) in dists.iter().enumerate() {
        let lx = left + 10.0;
        let ly = top + 14.0 + i as f64 * 18.0;
        svg.rect_f(lx, ly - 10.0, 12.0, 12.0, color_at(i), "");
        svg.text(lx + 16.0, ly, &dist.label, "#333", 10, "");
    }

    // Axis labels
    svg.text(
        left + plot_w / 2.0,
        (height - 6) as f64,
        "Score",
        "#333",
        11,
        r#"text-anchor="middle""#,
    );
    svg.text(
        14.0,
        top + plot_h / 2.0,
        "Count",
        "#333",
        11,
        &format!(
            r#"text-anchor="middle" transform="rotate(-90, 14, {:.1})""#,
            top + plot_h / 2.0
        ),
    );

    svg.render()
}

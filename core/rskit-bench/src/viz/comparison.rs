//! Branch comparison bar chart SVG rendering.

use super::svg::{Svg, color_at, draw_axes};
use crate::result::BranchResult;
use std::collections::{BTreeSet, HashMap};

const PAD_LEFT: usize = 60;
const PAD_TOP: usize = 50;
const PAD_RIGHT: usize = 20;
const PAD_BOTTOM: usize = 70;

/// Render a branch comparison grouped bar chart as a standalone SVG string.
pub fn render_comparison(
    branches: &HashMap<String, BranchResult>,
    width: usize,
    height: usize,
) -> String {
    if branches.is_empty() {
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
        "Branch Comparison",
        "#333",
        14,
        r#"text-anchor="middle" font-weight="bold""#,
    );

    // Axes
    draw_axes(&mut svg, PAD_LEFT, PAD_TOP, plot_w, plot_h);

    let mut branch_names: Vec<&String> = branches.keys().collect();
    branch_names.sort();

    let mut metric_set = BTreeSet::new();
    for br in branches.values() {
        for m in br.metrics.keys() {
            metric_set.insert(m.clone());
        }
    }
    let metric_names: Vec<String> = metric_set.into_iter().collect();

    if metric_names.is_empty() {
        return svg.render();
    }

    let mut max_val: f64 = 0.0;
    for br in branches.values() {
        for v in br.metrics.values() {
            if *v > max_val {
                max_val = *v;
            }
        }
    }
    if max_val <= 0.0 {
        max_val = 1.0;
    }

    // Y grid
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let y = top + plot_h - frac * plot_h;
        svg.line(left, y, left + plot_w, y, "#eee", 1.0, "");
        svg.text(
            left - 8.0,
            y + 4.0,
            &format!("{:.2}", frac * max_val),
            "#666",
            10,
            r#"text-anchor="end""#,
        );
    }

    let n_branches = branch_names.len();
    let n_metrics = metric_names.len();
    let group_width = plot_w / n_branches as f64;
    let bar_width = group_width / (n_metrics as f64 + 1.0);

    for (bi, bname) in branch_names.iter().enumerate() {
        let br = &branches[*bname];
        let group_x = left + bi as f64 * group_width;

        for (mi, mname) in metric_names.iter().enumerate() {
            let v = br.metrics.get(mname).copied().unwrap_or(0.0);
            let bar_h = (v / max_val) * plot_h;
            let x = group_x + mi as f64 * bar_width + bar_width * 0.15;
            let y = top + plot_h - bar_h;
            svg.rect_f(x, y, bar_width * 0.7, bar_h, color_at(mi), "");
        }

        svg.text(
            group_x + group_width / 2.0,
            top + plot_h + 18.0,
            bname,
            "#333",
            10,
            r#"text-anchor="middle""#,
        );
    }

    // Legend
    for (i, mname) in metric_names.iter().enumerate() {
        let lx = left + 10.0;
        let ly = top + 14.0 + i as f64 * 18.0;
        svg.rect_f(lx, ly - 10.0, 12.0, 12.0, color_at(i), "");
        svg.text(lx + 16.0, ly, mname, "#333", 10, "");
    }

    // Y axis label
    svg.text(
        14.0,
        top + plot_h / 2.0,
        "Value",
        "#333",
        11,
        &format!(
            r#"text-anchor="middle" transform="rotate(-90, 14, {:.1})""#,
            top + plot_h / 2.0
        ),
    );

    svg.render()
}

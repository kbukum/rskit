//! ASCII table report generation for terminal output.

use super::{Reporter, io_err};
use crate::result::BenchRunResult;
use rskit_errors::AppResult;
use std::io::Write;

/// Generates ASCII box-drawing tables for terminal output.
pub struct TableReporter;

impl Reporter for TableReporter {
    fn name(&self) -> &str {
        "table"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        // Header
        writeln!(
            w,
            "╔══════════════════════════════════════════════════════════════╗"
        )
        .map_err(io_err)?;
        writeln!(w, "║  BENCH RUN: {:<48}║", result.id).map_err(io_err)?;
        writeln!(
            w,
            "╠══════════════════════════════════════════════════════════════╣"
        )
        .map_err(io_err)?;
        writeln!(w, "║  Timestamp : {:<47}║", result.timestamp).map_err(io_err)?;
        if !result.tag.is_empty() {
            writeln!(w, "║  Tag       : {:<47}║", result.tag).map_err(io_err)?;
        }
        writeln!(
            w,
            "║  Dataset   : {:<47}║",
            format!("{} (v{})", result.dataset.name, result.dataset.version)
        )
        .map_err(io_err)?;
        writeln!(w, "║  Samples   : {:<47}║", result.dataset.sample_count).map_err(io_err)?;
        writeln!(
            w,
            "║  Duration  : {:<47}║",
            format!("{}ms", result.duration_ms)
        )
        .map_err(io_err)?;
        writeln!(
            w,
            "╚══════════════════════════════════════════════════════════════╝"
        )
        .map_err(io_err)?;
        writeln!(w).map_err(io_err)?;

        // Metrics table
        if !result.metrics.is_empty() {
            let name_width = result
                .metrics
                .iter()
                .map(|m| m.name.len())
                .max()
                .unwrap_or(10)
                .max(10);

            writeln!(w, "┌─{}─┬────────────┐", "─".repeat(name_width)).map_err(io_err)?;
            writeln!(
                w,
                "│ {:<nw$} │ {:>10} │",
                "Metric",
                "Value",
                nw = name_width
            )
            .map_err(io_err)?;
            writeln!(w, "├─{}─┼────────────┤", "─".repeat(name_width)).map_err(io_err)?;
            for m in &result.metrics {
                writeln!(
                    w,
                    "│ {:<nw$} │ {:>10.4} │",
                    m.name,
                    m.value,
                    nw = name_width
                )
                .map_err(io_err)?;
                for (k, v) in &m.values {
                    let sub_name = format!("  .{k}");
                    writeln!(w, "│ {sub_name:<name_width$} │ {v:>10.4} │").map_err(io_err)?;
                }
            }
            writeln!(w, "└─{}─┴────────────┘", "─".repeat(name_width)).map_err(io_err)?;
            writeln!(w).map_err(io_err)?;
        }

        // Branches
        if !result.branches.is_empty() {
            writeln!(
                w,
                "┌────────────────────┬──────┬────────┬────────┬──────────┐"
            )
            .map_err(io_err)?;
            writeln!(
                w,
                "│ Branch             │ Tier │   Avg+ │   Avg- │ Duration │"
            )
            .map_err(io_err)?;
            writeln!(
                w,
                "├────────────────────┼──────┼────────┼────────┼──────────┤"
            )
            .map_err(io_err)?;
            let mut branches: Vec<_> = result.branches.iter().collect();
            branches.sort_by_key(|(a, _)| *a);
            for (name, br) in &branches {
                writeln!(
                    w,
                    "│ {:<18} │ {:>4} │ {:>6.3} │ {:>6.3} │ {:>6}ms │",
                    truncate(name, 18),
                    br.tier,
                    br.avg_score_positive,
                    br.avg_score_negative,
                    br.duration_ms
                )
                .map_err(io_err)?;
            }
            writeln!(
                w,
                "└────────────────────┴──────┴────────┴────────┴──────────┘"
            )
            .map_err(io_err)?;
            writeln!(w).map_err(io_err)?;
        }

        // Sample summary
        if !result.samples.is_empty() {
            let correct = result.samples.iter().filter(|s| s.correct).count();
            let errors = result
                .samples
                .iter()
                .filter(|s| !s.error.is_empty())
                .count();
            let pct = 100.0 * correct as f64 / result.samples.len().max(1) as f64;
            writeln!(
                w,
                "Samples: {} total, {} correct ({:.1}%), {} errors",
                result.samples.len(),
                correct,
                pct,
                errors
            )
            .map_err(io_err)?;
        }

        Ok(())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

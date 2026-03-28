//! ASCII table report generation for terminal output.

use super::Reporter;
use crate::result::BenchRunResult;
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
        )?;
        writeln!(w, "║  BENCH RUN: {:<48}║", result.id)?;
        writeln!(
            w,
            "╠══════════════════════════════════════════════════════════════╣"
        )?;
        writeln!(w, "║  Timestamp : {:<47}║", result.timestamp)?;
        if !result.tag.is_empty() {
            writeln!(w, "║  Tag       : {:<47}║", result.tag)?;
        }
        writeln!(
            w,
            "║  Dataset   : {:<47}║",
            format!("{} (v{})", result.dataset.name, result.dataset.version)
        )?;
        writeln!(w, "║  Samples   : {:<47}║", result.dataset.sample_count)?;
        writeln!(
            w,
            "║  Duration  : {:<47}║",
            format!("{}ms", result.duration_ms)
        )?;
        writeln!(
            w,
            "╚══════════════════════════════════════════════════════════════╝"
        )?;
        writeln!(w)?;

        // Metrics table
        if !result.metrics.is_empty() {
            let name_width = result
                .metrics
                .iter()
                .map(|m| m.name.len())
                .max()
                .unwrap_or(10)
                .max(10);

            writeln!(w, "┌─{}─┬────────────┐", "─".repeat(name_width))?;
            writeln!(
                w,
                "│ {:<nw$} │ {:>10} │",
                "Metric",
                "Value",
                nw = name_width
            )?;
            writeln!(w, "├─{}─┼────────────┤", "─".repeat(name_width))?;
            for m in &result.metrics {
                writeln!(
                    w,
                    "│ {:<nw$} │ {:>10.4} │",
                    m.name,
                    m.value,
                    nw = name_width
                )?;
                for (k, v) in &m.values {
                    let sub_name = format!("  .{}", k);
                    writeln!(w, "│ {:<nw$} │ {:>10.4} │", sub_name, v, nw = name_width)?;
                }
            }
            writeln!(w, "└─{}─┴────────────┘", "─".repeat(name_width))?;
            writeln!(w)?;
        }

        // Branches
        if !result.branches.is_empty() {
            writeln!(
                w,
                "┌────────────────────┬──────┬────────┬────────┬──────────┐"
            )?;
            writeln!(
                w,
                "│ Branch             │ Tier │   Avg+ │   Avg- │ Duration │"
            )?;
            writeln!(
                w,
                "├────────────────────┼──────┼────────┼────────┼──────────┤"
            )?;
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
                )?;
            }
            writeln!(
                w,
                "└────────────────────┴──────┴────────┴────────┴──────────┘"
            )?;
            writeln!(w)?;
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
            )?;
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

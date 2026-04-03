//! Markdown report generation (GFM tables).

use super::{Reporter, io_err};
use crate::result::BenchRunResult;
use rskit_errors::AppResult;
use std::io::Write;

/// Generates GitHub-Flavored Markdown reports.
pub struct MarkdownReporter;

impl Reporter for MarkdownReporter {
    fn name(&self) -> &str {
        "markdown"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        // Header
        writeln!(w, "# Bench Run: {}", result.id).map_err(io_err)?;
        writeln!(w).map_err(io_err)?;
        writeln!(w, "| Field | Value |").map_err(io_err)?;
        writeln!(w, "|-------|-------|").map_err(io_err)?;
        writeln!(w, "| **Run ID** | `{}` |", result.id).map_err(io_err)?;
        writeln!(w, "| **Timestamp** | {} |", result.timestamp).map_err(io_err)?;
        if !result.tag.is_empty() {
            writeln!(w, "| **Tag** | {} |", result.tag).map_err(io_err)?;
        }
        writeln!(
            w,
            "| **Dataset** | {} (v{}) |",
            result.dataset.name, result.dataset.version
        )
        .map_err(io_err)?;
        writeln!(w, "| **Samples** | {} |", result.dataset.sample_count).map_err(io_err)?;
        writeln!(w, "| **Duration** | {}ms |", result.duration_ms).map_err(io_err)?;
        writeln!(w).map_err(io_err)?;

        // Metrics table
        if !result.metrics.is_empty() {
            writeln!(w, "## Metrics").map_err(io_err)?;
            writeln!(w).map_err(io_err)?;
            writeln!(w, "| Metric | Value |").map_err(io_err)?;
            writeln!(w, "|--------|------:|").map_err(io_err)?;
            for m in &result.metrics {
                writeln!(w, "| {} | {:.4} |", m.name, m.value).map_err(io_err)?;
                for (k, v) in &m.values {
                    writeln!(w, "| {}.{} | {:.4} |", m.name, k, v).map_err(io_err)?;
                }
            }
            writeln!(w).map_err(io_err)?;
        }

        // Confusion matrix (from metric detail)
        for m in &result.metrics {
            if let Some(ref detail) = m.detail {
                if let Some(labels) = detail.get("labels").and_then(|v| v.as_array()) {
                    if let Some(matrix) = detail.get("matrix").and_then(|v| v.as_array()) {
                        writeln!(w, "## Confusion Matrix").map_err(io_err)?;
                        writeln!(w).map_err(io_err)?;
                        write!(w, "| |").map_err(io_err)?;
                        for l in labels {
                            write!(w, " {} |", l.as_str().unwrap_or("?")).map_err(io_err)?;
                        }
                        writeln!(w).map_err(io_err)?;
                        write!(w, "|---|").map_err(io_err)?;
                        for _ in labels {
                            write!(w, "---:|").map_err(io_err)?;
                        }
                        writeln!(w).map_err(io_err)?;
                        for (i, row) in matrix.iter().enumerate() {
                            let label = labels.get(i).and_then(|v| v.as_str()).unwrap_or("?");
                            write!(w, "| **{label}** |").map_err(io_err)?;
                            if let Some(cells) = row.as_array() {
                                for cell in cells {
                                    write!(w, " {cell} |").map_err(io_err)?;
                                }
                            }
                            writeln!(w).map_err(io_err)?;
                        }
                        writeln!(w).map_err(io_err)?;
                    }
                }
            }
        }

        // Branches
        if !result.branches.is_empty() {
            writeln!(w, "## Branches").map_err(io_err)?;
            writeln!(w).map_err(io_err)?;
            writeln!(w, "| Branch | Tier | Avg+ | Avg- | Duration | Errors |").map_err(io_err)?;
            writeln!(w, "|--------|-----:|-----:|-----:|---------:|-------:|").map_err(io_err)?;
            let mut branches: Vec<_> = result.branches.iter().collect();
            branches.sort_by_key(|(a, _)| *a);
            for (name, br) in &branches {
                writeln!(
                    w,
                    "| {} | {} | {:.3} | {:.3} | {}ms | {} |",
                    name,
                    br.tier,
                    br.avg_score_positive,
                    br.avg_score_negative,
                    br.duration_ms,
                    br.errors
                )
                .map_err(io_err)?;
            }
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
            writeln!(w, "## Samples").map_err(io_err)?;
            writeln!(w).map_err(io_err)?;
            writeln!(
                w,
                "{} total, {} correct ({:.1}%), {} errors",
                result.samples.len(),
                correct,
                100.0 * correct as f64 / result.samples.len().max(1) as f64,
                errors
            )
            .map_err(io_err)?;
            writeln!(w).map_err(io_err)?;

            // Show incorrect samples
            let incorrect: Vec<_> = result.samples.iter().filter(|s| !s.correct).collect();
            if !incorrect.is_empty() {
                writeln!(w, "### Incorrect Predictions").map_err(io_err)?;
                writeln!(w).map_err(io_err)?;
                writeln!(w, "| Sample | Label | Predicted | Score |").map_err(io_err)?;
                writeln!(w, "|--------|-------|-----------|------:|").map_err(io_err)?;
                for s in &incorrect {
                    writeln!(
                        w,
                        "| {} | {} | {} | {:.3} |",
                        s.id, s.label, s.predicted, s.score
                    )
                    .map_err(io_err)?;
                }
                writeln!(w).map_err(io_err)?;
            }
        }

        Ok(())
    }
}

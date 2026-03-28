//! Markdown report generation (GFM tables).

use super::Reporter;
use rskit_errors::AppResult;
use crate::result::BenchRunResult;
use std::io::Write;

/// Generates GitHub-Flavored Markdown reports.
pub struct MarkdownReporter;

impl Reporter for MarkdownReporter {
    fn name(&self) -> &str {
        "markdown"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        // Header
        writeln!(w, "# Bench Run: {}", result.id)?;
        writeln!(w)?;
        writeln!(w, "| Field | Value |")?;
        writeln!(w, "|-------|-------|")?;
        writeln!(w, "| **Run ID** | `{}` |", result.id)?;
        writeln!(w, "| **Timestamp** | {} |", result.timestamp)?;
        if !result.tag.is_empty() {
            writeln!(w, "| **Tag** | {} |", result.tag)?;
        }
        writeln!(
            w,
            "| **Dataset** | {} (v{}) |",
            result.dataset.name, result.dataset.version
        )?;
        writeln!(w, "| **Samples** | {} |", result.dataset.sample_count)?;
        writeln!(w, "| **Duration** | {}ms |", result.duration_ms)?;
        writeln!(w)?;

        // Metrics table
        if !result.metrics.is_empty() {
            writeln!(w, "## Metrics")?;
            writeln!(w)?;
            writeln!(w, "| Metric | Value |")?;
            writeln!(w, "|--------|------:|")?;
            for m in &result.metrics {
                writeln!(w, "| {} | {:.4} |", m.name, m.value)?;
                for (k, v) in &m.values {
                    writeln!(w, "| {}.{} | {:.4} |", m.name, k, v)?;
                }
            }
            writeln!(w)?;
        }

        // Confusion matrix (from metric detail)
        for m in &result.metrics {
            if let Some(ref detail) = m.detail {
                if let Some(labels) = detail.get("labels").and_then(|v| v.as_array()) {
                    if let Some(matrix) = detail.get("matrix").and_then(|v| v.as_array()) {
                        writeln!(w, "## Confusion Matrix")?;
                        writeln!(w)?;
                        write!(w, "| |")?;
                        for l in labels {
                            write!(w, " {} |", l.as_str().unwrap_or("?"))?;
                        }
                        writeln!(w)?;
                        write!(w, "|---|")?;
                        for _ in labels {
                            write!(w, "---:|")?;
                        }
                        writeln!(w)?;
                        for (i, row) in matrix.iter().enumerate() {
                            let label = labels.get(i).and_then(|v| v.as_str()).unwrap_or("?");
                            write!(w, "| **{}** |", label)?;
                            if let Some(cells) = row.as_array() {
                                for cell in cells {
                                    write!(w, " {} |", cell)?;
                                }
                            }
                            writeln!(w)?;
                        }
                        writeln!(w)?;
                    }
                }
            }
        }

        // Branches
        if !result.branches.is_empty() {
            writeln!(w, "## Branches")?;
            writeln!(w)?;
            writeln!(w, "| Branch | Tier | Avg+ | Avg- | Duration | Errors |")?;
            writeln!(w, "|--------|-----:|-----:|-----:|---------:|-------:|")?;
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
                )?;
            }
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
            writeln!(w, "## Samples")?;
            writeln!(w)?;
            writeln!(
                w,
                "{} total, {} correct ({:.1}%), {} errors",
                result.samples.len(),
                correct,
                100.0 * correct as f64 / result.samples.len().max(1) as f64,
                errors
            )?;
            writeln!(w)?;

            // Show incorrect samples
            let incorrect: Vec<_> = result.samples.iter().filter(|s| !s.correct).collect();
            if !incorrect.is_empty() {
                writeln!(w, "### Incorrect Predictions")?;
                writeln!(w)?;
                writeln!(w, "| Sample | Label | Predicted | Score |")?;
                writeln!(w, "|--------|-------|-----------|------:|")?;
                for s in &incorrect {
                    writeln!(
                        w,
                        "| {} | {} | {} | {:.3} |",
                        s.id, s.label, s.predicted, s.score
                    )?;
                }
                writeln!(w)?;
            }
        }

        Ok(())
    }
}

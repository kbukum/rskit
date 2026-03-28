//! CSV report generation.

use super::Reporter;
use rskit_errors::AppResult;
use crate::result::BenchRunResult;
use std::io::Write;

/// Generates flat CSV with metric_name, value, details columns.
pub struct CsvReporter;

impl Reporter for CsvReporter {
    fn name(&self) -> &str {
        "csv"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        writeln!(w, "metric_name,value,details")?;

        for m in &result.metrics {
            let detail_str = match &m.detail {
                Some(d) => serde_json::to_string(d).unwrap_or_default(),
                None => String::new(),
            };
            writeln!(
                w,
                "{},{:.6},\"{}\"",
                csv_escape(&m.name),
                m.value,
                csv_escape(&detail_str)
            )?;
            for (k, v) in &m.values {
                writeln!(w, "{}.{},{:.6},", csv_escape(&m.name), csv_escape(k), v)?;
            }
        }

        // Branch metrics
        for (name, br) in &result.branches {
            for (mk, mv) in &br.metrics {
                writeln!(
                    w,
                    "branch.{}.{},{:.6},",
                    csv_escape(name),
                    csv_escape(mk),
                    mv
                )?;
            }
        }

        Ok(())
    }
}

fn csv_escape(s: &str) -> String {
    s.replace('"', "\"\"")
}

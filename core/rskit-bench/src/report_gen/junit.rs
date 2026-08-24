//! JUnit XML report generation.

use super::{Reporter, io_err};
use crate::result::BenchRunResult;
use rskit_errors::AppResult;
use std::io::Write;

/// Generates JUnit XML format. Metrics with targets become test cases.
pub struct JUnitReporter {
    suite_name: String,
}

impl JUnitReporter {
    /// Creates a JUnit reporter with the XML test suite name.
    pub fn new(suite_name: impl Into<String>) -> Self {
        Self {
            suite_name: suite_name.into(),
        }
    }
}

impl Default for JUnitReporter {
    fn default() -> Self {
        Self::new("bench")
    }
}

impl Reporter for JUnitReporter {
    fn name(&self) -> &str {
        "junit"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let total = result.metrics.len();
        let failures = 0usize;
        let duration_secs = result.duration_ms as f64 / 1000.0;

        writeln!(w, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").map_err(io_err)?;
        writeln!(
            w,
            "<testsuites tests=\"{total}\" failures=\"{failures}\" time=\"{duration_secs:.3}\">"
        )
        .map_err(io_err)?;
        writeln!(
            w,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" time=\"{:.3}\" timestamp=\"{}\">",
            xml_escape(&self.suite_name),
            total,
            failures,
            duration_secs,
            xml_escape(&result.timestamp)
        )
        .map_err(io_err)?;

        for m in &result.metrics {
            writeln!(
                w,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"0.000\">",
                xml_escape(&m.name),
                xml_escape(&self.suite_name)
            )
            .map_err(io_err)?;
            writeln!(w, "      <system-out>value={:.6}</system-out>", m.value).map_err(io_err)?;
            writeln!(w, "    </testcase>").map_err(io_err)?;
        }

        // Sample results as test cases
        for s in &result.samples {
            let classname = format!("{}.samples", self.suite_name);
            write!(
                w,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\">",
                xml_escape(&s.id),
                xml_escape(&classname),
                s.duration_ms as f64 / 1000.0
            )
            .map_err(io_err)?;
            if !s.correct {
                writeln!(w).map_err(io_err)?;
                writeln!(
                    w,
                    "      <failure message=\"expected={} predicted={} score={:.3}\"/>",
                    xml_escape(&s.label),
                    xml_escape(&s.predicted),
                    s.score
                )
                .map_err(io_err)?;
                write!(w, "    ").map_err(io_err)?;
            }
            writeln!(w, "</testcase>").map_err(io_err)?;
        }

        writeln!(w, "  </testsuite>").map_err(io_err)?;
        writeln!(w, "</testsuites>").map_err(io_err)?;

        Ok(())
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

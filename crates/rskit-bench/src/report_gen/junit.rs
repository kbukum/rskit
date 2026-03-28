//! JUnit XML report generation.

use super::Reporter;
use rskit_errors::AppResult;
use crate::result::BenchRunResult;
use std::io::Write;

/// Generates JUnit XML format. Metrics with targets become test cases.
pub struct JUnitReporter {
    suite_name: String,
}

impl JUnitReporter {
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
        let failures = 0usize; // No target-based failures in base implementation
        let duration_secs = result.duration_ms as f64 / 1000.0;

        writeln!(w, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
        writeln!(
            w,
            "<testsuites tests=\"{}\" failures=\"{}\" time=\"{:.3}\">",
            total, failures, duration_secs
        )?;
        writeln!(
            w,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" time=\"{:.3}\" timestamp=\"{}\">",
            xml_escape(&self.suite_name),
            total,
            failures,
            duration_secs,
            xml_escape(&result.timestamp)
        )?;

        for m in &result.metrics {
            writeln!(
                w,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"0.000\">",
                xml_escape(&m.name),
                xml_escape(&self.suite_name)
            )?;
            // Emit value as system-out
            writeln!(w, "      <system-out>value={:.6}</system-out>", m.value)?;
            writeln!(w, "    </testcase>")?;
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
            )?;
            if !s.correct {
                writeln!(w)?;
                writeln!(
                    w,
                    "      <failure message=\"expected={} predicted={} score={:.3}\"/>",
                    xml_escape(&s.label),
                    xml_escape(&s.predicted),
                    s.score
                )?;
                write!(w, "    ")?;
            }
            writeln!(w, "</testcase>")?;
        }

        writeln!(w, "  </testsuite>")?;
        writeln!(w, "</testsuites>")?;

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

//! Self-contained HTML report generation.
//!
//! Produces a single browsable HTML document: summary, metrics, charts,
//! branch comparison, and per-sample tables. Charts reuse the crate's existing
//! [`vegalite_specs`] — each spec is embedded as an
//! inert `application/json` `<script>` block and rendered client-side by
//! Vega-Embed, never interpolated as executable markup.
//!
//! # Security
//!
//! Every interpolated value (ids, tags, labels, metric/branch/sample fields) is
//! HTML-escaped at the boundary via [`escape`], so a malicious sample label
//! such as `<script>` renders as inert text rather than active markup. The
//! Vega/Vega-Lite/Vega-Embed runtimes load from CDN with **explicitly pinned
//! major versions**; chart specs are consumed as JSON data, so they cannot
//! introduce script execution.

use super::{Reporter, io_err, vegalite_specs};
use crate::result::BenchRunResult;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::BTreeMap;
use std::io::Write;

/// Generates a self-contained HTML report with embedded Vega-Lite charts.
pub struct HtmlReporter;

impl Reporter for HtmlReporter {
    fn name(&self) -> &str {
        "html"
    }

    fn generate(&self, w: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        let specs = vegalite_specs(result);
        let charts = specs.as_object().map_or_else(BTreeMap::new, |m| {
            m.iter().map(|(k, v)| (k.clone(), v)).collect()
        });

        let mut out = String::new();
        out.push_str(&head(result));
        out.push_str(&summary(result));
        out.push_str(&metrics(result));
        out.push_str(&charts_section(&charts)?);
        out.push_str(&branches(result));
        out.push_str(&samples(result));
        out.push_str(&footer(&charts));

        write!(w, "{out}").map_err(io_err)?;
        Ok(())
    }
}

/// Escapes the five HTML-significant characters (`& < > " '`).
///
/// The bench crate has no canonical HTML escaper (see
/// [`docs/CONCERN-OWNERS.md`](../../../../docs/CONCERN-OWNERS.md): no owner for
/// HTML escaping), and this reporter is the only consumer, so a minimal local
/// escaper is kept here rather than introducing a dependency for one call site.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Turns a spec key (`score_distribution`) into a chart title (`score distribution`).
fn chart_title(key: &str) -> String {
    key.replace('_', " ")
}

fn head(result: &BenchRunResult) -> String {
    let mut title = "Bench Report".to_string();
    if !result.tag.is_empty() {
        title.push_str(" — ");
        title.push_str(&escape(&result.tag));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
<script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
<script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #f5f5f5; color: #333; }}
  header {{ background: #1a1a2e; color: #fff; padding: 1.5rem 2rem; }}
  header h1 {{ font-size: 1.5rem; font-weight: 600; }}
  header p {{ font-size: 0.875rem; opacity: 0.8; margin-top: 0.25rem; }}
  .container {{ max-width: 1200px; margin: 0 auto; padding: 1.5rem; }}
  section {{ margin-bottom: 1.5rem; }}
  section h2 {{ font-size: 1.25rem; margin-bottom: 0.75rem; color: #1a1a2e; border-bottom: 2px solid #e0e0e0; padding-bottom: 0.25rem; }}
  .card {{ background: #fff; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); padding: 1.25rem; margin-bottom: 1rem; }}
  table {{ width: 100%; border-collapse: collapse; }}
  th, td {{ text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid #eee; }}
  th {{ font-weight: 600; font-size: 0.8rem; text-transform: uppercase; color: #666; }}
  td {{ font-size: 0.9rem; }}
  .chart-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(450px, 1fr)); gap: 1rem; }}
  .chart-card {{ background: #fff; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); padding: 1rem; }}
  .chart-card h3 {{ font-size: 0.95rem; margin-bottom: 0.5rem; color: #444; }}
  .samples-table {{ max-height: 400px; overflow-y: auto; }}
  .correct {{ color: #2e7d32; }}
  .incorrect {{ color: #c62828; }}
  .badge {{ display: inline-block; padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600; }}
  .badge-ok {{ background: #e8f5e9; color: #2e7d32; }}
  .badge-err {{ background: #ffebee; color: #c62828; }}
</style>
</head>
<body>
<header>
  <h1>{title}</h1>
  <p>Run {id} &middot; {timestamp} &middot; {duration}ms</p>
</header>
<div class="container">
"#,
        title = title,
        id = escape(&result.id),
        timestamp = escape(&result.timestamp),
        duration = result.duration_ms,
    )
}

fn summary(result: &BenchRunResult) -> String {
    let mut b = String::new();
    b.push_str(r#"<section><h2>Summary</h2><div class="card"><table>"#);
    b.push_str(&format!(
        "<tr><th>Dataset</th><td>{} v{}</td></tr>",
        escape(&result.dataset.name),
        escape(&result.dataset.version)
    ));
    b.push_str(&format!(
        "<tr><th>Samples</th><td>{}</td></tr>",
        result.dataset.sample_count
    ));

    if !result.dataset.label_distribution.is_empty() {
        let ordered: BTreeMap<_, _> = result.dataset.label_distribution.iter().collect();
        let parts: Vec<String> = ordered
            .iter()
            .map(|(l, c)| format!("{}: {}", escape(l), c))
            .collect();
        b.push_str(&format!(
            "<tr><th>Labels</th><td>{}</td></tr>",
            parts.join(", ")
        ));
    }

    if !result.samples.is_empty() {
        let correct = result.samples.iter().filter(|s| s.correct).count();
        let pct = 100.0 * correct as f64 / result.samples.len() as f64;
        b.push_str(&format!(
            "<tr><th>Accuracy</th><td>{} / {} ({:.1}%)</td></tr>",
            correct,
            result.samples.len(),
            pct
        ));
    }

    b.push_str("</table></div></section>");
    b
}

fn metrics(result: &BenchRunResult) -> String {
    if result.metrics.is_empty() {
        return String::new();
    }
    let mut b = String::new();
    b.push_str(r#"<section><h2>Metrics</h2><div class="card"><table>"#);
    b.push_str("<tr><th>Metric</th><th>Value</th><th>Per-Label</th></tr>");
    for m in &result.metrics {
        let per_label = if m.values.is_empty() {
            "—".to_string()
        } else {
            let ordered: BTreeMap<_, _> = m.values.iter().collect();
            ordered
                .iter()
                .map(|(k, v)| format!("{}: {:.4}", escape(k), v))
                .collect::<Vec<_>>()
                .join(", ")
        };
        b.push_str(&format!(
            "<tr><td>{}</td><td>{:.4}</td><td>{}</td></tr>",
            escape(&m.name),
            m.value,
            per_label
        ));
    }
    b.push_str("</table></div></section>");
    b
}

fn charts_section(charts: &BTreeMap<String, &serde_json::Value>) -> AppResult<String> {
    if charts.is_empty() {
        return Ok(String::new());
    }
    let mut b = String::new();
    b.push_str(r#"<section><h2>Charts</h2><div class="chart-grid">"#);
    for (i, (key, spec)) in charts.iter().enumerate() {
        let chart_id = format!("chart-{i}");
        b.push_str(&format!(
            r#"<div class="chart-card"><h3>{}</h3><div id="{}"></div>"#,
            escape(&chart_title(key)),
            chart_id
        ));
        let spec_json = serde_json::to_string(spec)
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("serialize: {e}")))?;
        b.push_str(&format!(
            r#"<script type="application/json" id="{chart_id}-spec">{spec_json}</script>"#
        ));
        b.push_str("</div>");
    }
    b.push_str("</div></section>");
    Ok(b)
}

fn branches(result: &BenchRunResult) -> String {
    if result.branches.is_empty() {
        return String::new();
    }
    let ordered: BTreeMap<_, _> = result.branches.iter().collect();

    let mut metric_keys: BTreeMap<&str, ()> = BTreeMap::new();
    for br in ordered.values() {
        for mk in br.metrics.keys() {
            metric_keys.insert(mk.as_str(), ());
        }
    }

    let mut b = String::new();
    b.push_str(r#"<section><h2>Branch Comparison</h2><div class="card"><table>"#);
    b.push_str("<tr><th>Branch</th><th>Tier</th>");
    for mk in metric_keys.keys() {
        b.push_str(&format!("<th>{}</th>", escape(mk)));
    }
    b.push_str("<th>Avg+</th><th>Avg−</th><th>Duration</th><th>Errors</th></tr>");

    for br in ordered.values() {
        b.push_str(&format!(
            "<tr><td>{}</td><td>{}</td>",
            escape(&br.name),
            br.tier
        ));
        for mk in metric_keys.keys() {
            let v = br.metrics.get(*mk).copied().unwrap_or(0.0);
            b.push_str(&format!("<td>{v:.4}</td>"));
        }
        b.push_str(&format!(
            "<td>{:.4}</td><td>{:.4}</td><td>{}ms</td><td>{}</td></tr>",
            br.avg_score_positive, br.avg_score_negative, br.duration_ms, br.errors
        ));
    }

    b.push_str("</table></div></section>");
    b
}

fn samples(result: &BenchRunResult) -> String {
    if result.samples.is_empty() {
        return String::new();
    }
    let mut b = String::new();
    b.push_str(r#"<section><h2>Sample Details</h2><div class="card samples-table"><table>"#);
    b.push_str(
        "<tr><th>ID</th><th>Label</th><th>Predicted</th><th>Score</th><th>Correct</th><th>Duration</th><th>Error</th></tr>",
    );

    for s in &result.samples {
        let (correct_class, correct_badge, correct_text) = if s.correct {
            ("correct", "badge-ok", "✓")
        } else {
            ("incorrect", "badge-err", "✗")
        };
        let err_text = if s.error.is_empty() {
            "—".to_string()
        } else {
            escape(&s.error)
        };
        b.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td class="{}"><span class="badge {}">{}</span></td><td>{}ms</td><td>{}</td></tr>"#,
            escape(&s.id),
            escape(&s.label),
            escape(&s.predicted),
            s.score,
            correct_class,
            correct_badge,
            correct_text,
            s.duration_ms,
            err_text
        ));
    }

    b.push_str("</table></div></section>");
    b
}

fn footer(charts: &BTreeMap<String, &serde_json::Value>) -> String {
    let mut b = String::new();
    if !charts.is_empty() {
        b.push_str("<script>\n");
        b.push_str("document.addEventListener('DOMContentLoaded', function() {\n");
        for i in 0..charts.len() {
            let chart_id = format!("chart-{i}");
            b.push_str(&format!(
                "  var spec{i} = JSON.parse(document.getElementById('{chart_id}-spec').textContent);\n  vegaEmbed('#{chart_id}', spec{i}, {{actions: false}}).catch(console.error);\n"
            ));
        }
        b.push_str("});\n");
        b.push_str("</script>\n");
    }
    b.push_str("</div>\n</body>\n</html>\n");
    b
}

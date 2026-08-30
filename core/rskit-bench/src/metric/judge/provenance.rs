//! Judge-identity detail keys and extraction of judge provenance from a result set.

use crate::MetricResult;

/// Detail key recording the judge provider in a [`MetricResult`], lifted into run provenance.
pub(crate) const DETAIL_JUDGE_PROVIDER: &str = "judge_provider";
/// Detail key recording the judge model in a [`MetricResult`], lifted into run provenance.
pub(crate) const DETAIL_JUDGE_MODEL: &str = "judge_model";
/// Detail key recording the model the provider reported as actually generating the verdicts, when it differs from — or refines — the requested model, so an alias or backend routing is visible in provenance.
pub(crate) const DETAIL_JUDGE_RESOLVED_MODEL: &str = "judge_resolved_model";
/// Detail key recording the judge prompt id in a [`MetricResult`], lifted into run provenance.
pub(crate) const DETAIL_JUDGE_PROMPT_ID: &str = "judge_prompt_id";
/// Detail key recording the judge prompt version in a [`MetricResult`], lifted into run provenance.
pub(crate) const DETAIL_JUDGE_PROMPT_VERSION: &str = "judge_prompt_version";
/// Detail key recording the judge prompt rubric fingerprint in a [`MetricResult`], lifted into run provenance.
pub(crate) const DETAIL_JUDGE_PROMPT_FINGERPRINT: &str = "judge_prompt_fingerprint";

/// Extracts the judge identities recorded by [`llm_judge`](super::llm_judge) metrics from a result set into judge provenance records.
///
/// Records every judge metric present in the result set, keyed by metric name; returns an empty map when the suite ran no judge metric.
pub(crate) fn judges_from_results(
    results: &[MetricResult],
) -> std::collections::BTreeMap<String, crate::provenance::JudgeProvenance> {
    let mut judges = std::collections::BTreeMap::new();
    for result in results {
        let Some(detail) = &result.detail else {
            continue;
        };
        let detail_str = |key: &str| detail.get(key).and_then(serde_json::Value::as_str);
        if let (
            Some(provider),
            Some(model),
            Some(prompt_id),
            Some(prompt_version),
            Some(prompt_fingerprint),
        ) = (
            detail_str(DETAIL_JUDGE_PROVIDER),
            detail_str(DETAIL_JUDGE_MODEL),
            detail_str(DETAIL_JUDGE_PROMPT_ID),
            detail_str(DETAIL_JUDGE_PROMPT_VERSION),
            detail_str(DETAIL_JUDGE_PROMPT_FINGERPRINT),
        ) {
            let mut provenance = crate::provenance::JudgeProvenance::new(
                provider,
                model,
                prompt_id,
                prompt_version,
                prompt_fingerprint,
            );
            if let Some(resolved) = detail_str(DETAIL_JUDGE_RESOLVED_MODEL) {
                provenance = provenance.with_resolved_model(resolved);
            }
            judges.insert(result.name.clone(), provenance);
        }
    }
    judges
}

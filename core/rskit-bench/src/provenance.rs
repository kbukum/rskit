//! Reproducibility provenance for benchmark runs.
//!
//! A [`RunProvenance`] record captures everything needed to reproduce and audit a benchmark run — the deterministic seed and RNG algorithm, the source-control commit, the tool and host identity, and an order-independent content hash of the evaluated dataset. Host and commit values are gathered through an injected [`ProvenanceProbe`], so unit tests can supply fixed values with no process, environment, or network access.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use rskit_util::hash::ContentHasher;

use crate::types::BenchSample;

/// Provenance metadata for an LLM judge metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JudgeProvenance {
    /// Provider that served the judge model.
    pub provider: String,
    /// Model name used by the judge.
    pub model: String,
    /// Stable identifier of the versioned judge prompt.
    pub prompt_id: String,
    /// Version of the judge prompt, so scores map to the exact prompt revision that produced them.
    pub prompt_version: String,
    /// Content fingerprint of the complete rubric (template body plus system instruction), so scores only ever compare against an identical rubric even under the same prompt id and version.
    pub prompt_fingerprint: String,
    /// Model the provider reported as actually generating the verdicts, when it differs from the requested [`model`](Self::model) (an alias or backend route). `None` when the provider reported no model or reported the requested one, so a run records the true scoring model rather than only the requested name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_model: Option<String>,
}

impl JudgeProvenance {
    /// Creates a new judge provenance record.
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        prompt_id: impl Into<String>,
        prompt_version: impl Into<String>,
        prompt_fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            prompt_id: prompt_id.into(),
            prompt_version: prompt_version.into(),
            prompt_fingerprint: prompt_fingerprint.into(),
            resolved_model: None,
        }
    }

    /// Records the model the provider reported as actually generating the verdicts, when it differs from the requested model.
    #[must_use]
    pub fn with_resolved_model(mut self, resolved_model: impl Into<String>) -> Self {
        self.resolved_model = Some(resolved_model.into());
        self
    }
}

/// Reproducibility metadata captured for a benchmark run.
///
/// The seed always serializes — it drives reproducibility, so a run must record which seed produced it even when it is zero. Genuinely-absent fields (an unresolved commit, an unnamed dataset) are omitted so the record stays sparse rather than padded with empty placeholders.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RunProvenance {
    /// Deterministic run seed (see [`RunOptions::with_seed`](crate::RunOptions::with_seed)).
    #[serde(default)]
    pub seed: u64,
    /// RNG algorithm the seed drives (see [`RNG_ALGORITHM`](crate::RNG_ALGORITHM)), so a seed maps to the same sequence across rebuilds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rng_algorithm: String,
    /// Source-control commit the run was built from, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Version of the bench tool that produced the run.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_version: String,
    /// Host name the run executed on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    /// Operating system the run executed on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub os: String,
    /// CPU architecture the run executed on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arch: String,
    /// Order-independent content hash of the evaluated dataset.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dataset_hash: String,
    /// Dataset name from the manifest.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dataset_name: String,
    /// Dataset version from the manifest.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dataset_version: String,
    /// Evaluator branch names, in registration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
    /// Metric names computed for the run, in suite order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<String>,
    /// Judge identities for LLM-judge metrics, keyed by metric name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub judges: BTreeMap<String, JudgeProvenance>,
}

/// Gathers host and source-control provenance for a benchmark run.
///
/// Injected into the [`BenchRunner`](crate::BenchRunner) so tests supply deterministic values with no process, environment, or network access.
pub trait ProvenanceProbe: Send + Sync {
    /// Source-control commit for the run, or `None` when unresolvable.
    fn git_commit(&self) -> Option<String>;
    /// Host name the run executes on.
    fn host(&self) -> String;
    /// Operating system identifier (for example `std::env::consts::OS`).
    fn os(&self) -> String;
    /// CPU architecture identifier (for example `std::env::consts::ARCH`).
    fn arch(&self) -> String;
}

/// Environment variables inspected, in precedence order, for the run commit.
const GIT_COMMIT_ENV_VARS: [&str; 4] =
    ["GITHUB_SHA", "GIT_COMMIT", "CI_COMMIT_SHA", "SOURCE_COMMIT"];

/// Default probe: reads host/os/arch from the standard library and the git commit from well-known CI environment variables.
///
/// The commit is resolved best-effort from CI environment variables rather than by invoking `git`, so the bench crate takes no dependency on a git library and the probe performs no process or network I/O. A caller that wants an authoritative commit (for example via `rskit-git`) can resolve it and inject a [`FixedProvenanceProbe`] instead.
#[derive(Debug, Default, Clone)]
pub struct SystemProvenanceProbe;

impl ProvenanceProbe for SystemProvenanceProbe {
    fn git_commit(&self) -> Option<String> {
        GIT_COMMIT_ENV_VARS.iter().find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    }

    fn host(&self) -> String {
        for key in ["HOSTNAME", "COMPUTERNAME", "HOST"] {
            if let Ok(value) = std::env::var(key) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        "unknown".to_string()
    }

    fn os(&self) -> String {
        std::env::consts::OS.to_string()
    }

    fn arch(&self) -> String {
        std::env::consts::ARCH.to_string()
    }
}

/// Deterministic probe for tests and reproducible fixtures: returns fixed, injected
/// values and performs no environment or process access.
#[derive(Debug, Clone, Default)]
pub struct FixedProvenanceProbe {
    git_commit: Option<String>,
    host: String,
    os: String,
    arch: String,
}

impl FixedProvenanceProbe {
    /// Creates a fixed probe with all fields empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the git commit the probe reports.
    #[must_use]
    pub fn with_git_commit(mut self, commit: impl Into<String>) -> Self {
        self.git_commit = Some(commit.into());
        self
    }

    /// Sets the host name the probe reports.
    #[must_use]
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Sets the operating system identifier the probe reports.
    #[must_use]
    pub fn with_os(mut self, os: impl Into<String>) -> Self {
        self.os = os.into();
        self
    }

    /// Sets the CPU architecture identifier the probe reports.
    #[must_use]
    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.arch = arch.into();
        self
    }
}

impl ProvenanceProbe for FixedProvenanceProbe {
    fn git_commit(&self) -> Option<String> {
        self.git_commit.clone()
    }

    fn host(&self) -> String {
        self.host.clone()
    }

    fn os(&self) -> String {
        self.os.clone()
    }

    fn arch(&self) -> String {
        self.arch.clone()
    }
}

/// Computes an order-independent content hash of a dataset from each sample's id, raw input bytes, and label, so the same dataset hashes identically regardless of load order.
///
/// Each field is folded incrementally with length-prefixed framing via [`ContentHasher::update_framed`], so datasets whose ids, inputs, or labels contain tabs, newlines, or other delimiters cannot collide, and no large intermediate buffer is materialized regardless of dataset size. Input bytes are included so that changing a sample's content while keeping its id and label still changes the hash.
pub(crate) fn dataset_hash<L: std::fmt::Display>(samples: &[BenchSample<L>]) -> String {
    let mut records: Vec<(&str, &[u8], String)> = samples
        .iter()
        .map(|sample| {
            (
                sample.id.as_str(),
                sample.input.as_slice(),
                sample.label.to_string(),
            )
        })
        .collect();
    records.sort();
    let mut hasher = ContentHasher::new();
    for (id, input, label) in &records {
        hasher.update_framed(b"id", id.as_bytes());
        hasher.update_framed(b"input", input);
        hasher.update_framed(b"label", label.as_bytes());
    }
    hasher.finalize_hex()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn sample(id: &str, label: &str) -> BenchSample<String> {
        sample_with_input(id, label, b"")
    }

    fn sample_with_input(id: &str, label: &str, input: &[u8]) -> BenchSample<String> {
        BenchSample {
            id: id.to_string(),
            input: input.to_vec(),
            label: label.to_string(),
            source: String::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn default_provenance_records_the_seed() {
        let provenance = RunProvenance::default();
        let json = serde_json::to_string(&provenance).expect("serialize");
        assert_eq!(json, "{\"seed\":0}");
    }

    #[test]
    fn populated_provenance_round_trips() {
        let mut judges = BTreeMap::new();
        judges.insert(
            "llm_judge[openai/gpt-judge@rskit.builtin.judge@1.0.0]".to_string(),
            JudgeProvenance::new(
                "openai",
                "gpt-judge",
                "rskit.builtin.judge",
                "1.0.0",
                "0123456789abcdef",
            ),
        );
        let provenance = RunProvenance {
            seed: 42,
            rng_algorithm: "rand_chacha:ChaCha8Rng".into(),
            git_commit: Some("abc123".into()),
            tool_version: "0.2.0".into(),
            host: "ci-runner".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            dataset_hash: "deadbeef".into(),
            dataset_name: "eval".into(),
            dataset_version: "2.1.0".into(),
            branches: vec!["primary".into()],
            metrics: vec!["exact_match".into()],
            judges,
            ..Default::default()
        };
        let json = serde_json::to_string(&provenance).expect("serialize");
        let restored: RunProvenance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(provenance, restored);
    }

    #[test]
    fn fixed_probe_reports_injected_values() {
        let probe = FixedProvenanceProbe::new()
            .with_git_commit("feedface")
            .with_host("test-host")
            .with_os("linux")
            .with_arch("aarch64");
        assert_eq!(probe.git_commit(), Some("feedface".to_string()));
        assert_eq!(probe.host(), "test-host");
        assert_eq!(probe.os(), "linux");
        assert_eq!(probe.arch(), "aarch64");
    }

    #[test]
    fn fixed_probe_without_commit_reports_none() {
        let probe = FixedProvenanceProbe::new().with_host("h");
        assert_eq!(probe.git_commit(), None);
    }

    #[test]
    fn dataset_hash_is_order_independent() {
        let forward = [sample("a", "yes"), sample("b", "no")];
        let reversed = [sample("b", "no"), sample("a", "yes")];
        assert_eq!(dataset_hash(&forward), dataset_hash(&reversed));
    }

    #[test]
    fn dataset_hash_changes_with_content() {
        let base = [sample("a", "yes")];
        let changed = [sample("a", "no")];
        assert_ne!(dataset_hash(&base), dataset_hash(&changed));
    }

    #[test]
    fn dataset_hash_changes_when_only_input_changes() {
        // Same id and label, different raw input content must hash differently —
        // evaluators consume `input`, so it is part of dataset identity.
        let base = [sample_with_input("a", "yes", b"first")];
        let changed = [sample_with_input("a", "yes", b"second")];
        assert_ne!(dataset_hash(&base), dataset_hash(&changed));
    }

    #[test]
    fn dataset_hash_resists_delimiter_collision() {
        // Ids/labels containing the delimiters a naive join would use must not
        // alias distinct datasets.
        let tab = [sample("a\tb", "c")];
        let split = [sample("a", "b\tc")];
        assert_ne!(dataset_hash(&tab), dataset_hash(&split));

        let newline = [sample("a", "b"), sample("c", "d")];
        let merged = [sample("a", "b\nc"), sample("", "d")];
        assert_ne!(dataset_hash(&newline), dataset_hash(&merged));
    }
}

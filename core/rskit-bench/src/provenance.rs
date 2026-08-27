//! Reproducibility provenance for benchmark runs.
//!
//! A [`RunProvenance`] record captures everything needed to reproduce and audit a
//! benchmark run — the deterministic seed, the source-control commit, the tool and
//! host identity, and an order-independent content hash of the evaluated dataset.
//! Host and commit values are gathered through an injected [`ProvenanceProbe`], so
//! unit tests can supply fixed values with no process, environment, or network
//! access.

use serde::{Deserialize, Serialize};

use crate::types::BenchSample;

/// Reproducibility metadata captured for a benchmark run.
///
/// Fields are individually omitted from serialization when empty so a run built
/// without a probe (for example a hand-constructed fixture) serializes exactly as
/// before this record existed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunProvenance {
    /// Deterministic run seed (see [`RunOptions::with_seed`](crate::RunOptions::with_seed)).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seed: u64,
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
    /// Dataset name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dataset_name: String,
    /// Evaluator branch names, in registration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
    /// Metric names computed for the run, in suite order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl RunProvenance {
    /// Returns `true` when no provenance has been recorded (all fields default).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Gathers host and source-control provenance for a benchmark run.
///
/// Injected into the [`BenchRunner`](crate::BenchRunner) so tests supply
/// deterministic values with no process, environment, or network access.
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

/// Default probe: reads host/os/arch from the standard library and the git commit
/// from well-known CI environment variables.
///
/// The commit is resolved best-effort from CI environment variables rather than by
/// invoking `git`, so the bench crate takes no dependency on a git library and the
/// probe performs no process or network I/O. A caller that wants an authoritative
/// commit (for example via `rskit-git`) can resolve it and inject a
/// [`FixedProvenanceProbe`] instead.
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

/// Computes an order-independent SHA-256 content hash of a dataset from each
/// sample's id and label, so the same dataset hashes identically regardless of load
/// order.
pub(crate) fn dataset_hash<L: std::fmt::Display>(samples: &[BenchSample<L>]) -> String {
    let mut lines: Vec<String> = samples
        .iter()
        .map(|sample| format!("{}\t{}", sample.id, sample.label))
        .collect();
    lines.sort();
    rskit_util::hash::sha256::sha256_hex(lines.join("\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn sample(id: &str, label: &str) -> BenchSample<String> {
        BenchSample {
            id: id.to_string(),
            input: Vec::new(),
            label: label.to_string(),
            source: String::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn empty_provenance_serializes_to_empty_object() {
        let provenance = RunProvenance::default();
        assert!(provenance.is_empty());
        let json = serde_json::to_string(&provenance).expect("serialize");
        assert_eq!(json, "{}");
    }

    #[test]
    fn populated_provenance_round_trips() {
        let provenance = RunProvenance {
            seed: 42,
            git_commit: Some("abc123".into()),
            tool_version: "0.2.0".into(),
            host: "ci-runner".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            dataset_hash: "deadbeef".into(),
            dataset_name: "eval".into(),
            branches: vec!["primary".into()],
            metrics: vec!["exact_match".into()],
        };
        assert!(!provenance.is_empty());
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
}

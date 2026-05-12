//! Option types for git operations.

use std::time::SystemTime;

use crate::types::Signature;

/// Raw extra CLI-style arguments preserved for backend-specific usage.
pub type ExtraArgs = Vec<String>;

/// Controls log traversal.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Maximum number of commits to return.
    pub max_count: Option<usize>,
    /// Optional path to filter history by.
    pub path_filter: Option<String>,
    /// Optional author name or email filter.
    pub author_filter: Option<String>,
    /// Lower inclusive time bound.
    pub since: Option<SystemTime>,
    /// Upper inclusive time bound.
    pub until: Option<SystemTime>,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls blame output.
#[derive(Debug, Clone, Default)]
pub struct BlameOptions {
    /// First one-based line to include.
    pub start_line: Option<usize>,
    /// Last one-based line to include.
    pub end_line: Option<usize>,
    /// Whether to ignore whitespace-only changes.
    pub ignore_whitespace: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls `git describe`-style inspection.
#[derive(Debug, Clone, Default)]
pub struct DescribeOptions {
    /// Restrict matching to annotated tags.
    pub annotated_tags_only: bool,
    /// Include long format output even when on an exact tag.
    pub long: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls `git grep`-style inspection.
#[derive(Debug, Clone, Default)]
pub struct GrepOptions {
    /// Limit matches to the given repository-relative paths.
    pub pathspecs: Vec<String>,
    /// Whether matching should ignore case.
    pub ignore_case: bool,
    /// Whether to include line numbers in backend command composition.
    pub line_numbers: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls commit creation.
#[derive(Debug, Clone, Default)]
pub struct CommitOptions {
    /// Explicit author signature.
    pub author: Option<Signature>,
    /// Explicit committer signature.
    pub committer: Option<Signature>,
    /// Whether to create a signed commit.
    pub sign: bool,
    /// Whether to amend the current commit.
    pub amend: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls merge behavior.
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    /// Prefer a merge commit even when fast-forward is possible.
    pub no_fast_forward: bool,
    /// Perform a squash merge.
    pub squash: bool,
    /// Optional merge commit message.
    pub message: Option<String>,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls rebase behavior.
#[derive(Debug, Clone, Default)]
pub struct RebaseOptions {
    /// Whether to request an interactive rebase.
    pub interactive: bool,
    /// Whether to enable autosquash semantics.
    pub autosquash: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls cherry-pick behavior.
#[derive(Debug, Clone, Default)]
pub struct CherryPickOptions {
    /// Mainline parent to use for merge commits.
    pub mainline: Option<usize>,
    /// Apply changes without committing.
    pub no_commit: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls checkout behavior.
#[derive(Debug, Clone, Default)]
pub struct CheckoutOptions {
    /// Force checkout when local changes are present.
    pub force: bool,
    /// Create a new branch while checking out.
    pub create_branch: Option<String>,
    /// Detach HEAD at the target ref.
    pub detach: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls fetch behavior.
#[derive(Debug, Clone, Default)]
pub struct FetchOptions {
    /// Whether to prune deleted remote refs.
    pub prune: bool,
    /// Optional shallow fetch depth.
    pub depth: Option<usize>,
    /// Refspec overrides.
    pub refspecs: Vec<String>,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls push behavior.
#[derive(Debug, Clone, Default)]
pub struct PushOptions {
    /// Whether to force push.
    pub force: bool,
    /// Refspec overrides.
    pub refspecs: Vec<String>,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

/// Controls repository cleaning operations.
#[derive(Debug, Clone, Default)]
pub struct CleanOptions {
    /// Remove untracked directories in addition to files.
    pub directories: bool,
    /// Include ignored files.
    pub ignored: bool,
    /// Require force semantics for destructive cleanup.
    pub force: bool,
    /// Backend-specific passthrough arguments.
    pub extra_args: ExtraArgs,
}

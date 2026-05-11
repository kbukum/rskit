//! Write operations for the CLI backend.

use std::process::Command;

use rskit_errors::{AppError, AppResult};

use crate::options::{CheckoutOptions, CherryPickOptions, MergeOptions, RebaseOptions};
use crate::read::Inspector;
use crate::types::{MergeResult, Oid, RebaseResult, ResetMode, StashEntry};
use crate::write::{CheckoutManager, CherryPicker, Merger, Rebaser, Resetter, Stasher};

use super::Backend;

impl Merger for Backend {
    fn merge(&self, branch: &str, opts: Option<&MergeOptions>) -> AppResult<MergeResult> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["merge".to_string()];
        if opts.no_fast_forward {
            args.push("--no-ff".to_string());
        }
        if opts.squash {
            args.push("--squash".to_string());
        }
        if let Some(message) = opts.message {
            args.push("-m".to_string());
            args.push(message);
        }
        args.extend(opts.extra_args);
        args.push("--".to_string());
        args.push(branch.to_string());

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        match self.run(&refs) {
            Ok(output) => {
                let head = self.rev_parse("HEAD").ok();
                let stdout = String::from_utf8_lossy(&output);
                let fast_forward = stdout.contains("Fast-forward");
                Ok(MergeResult {
                    head,
                    fast_forward,
                    conflicts: Vec::new(),
                })
            }
            Err(err) => match conflict_stderr(&err) {
                Some(stderr) if stderr.contains("CONFLICT") => Ok(MergeResult {
                    head: None,
                    fast_forward: false,
                    conflicts: parse_conflict_paths(self),
                }),
                _ => Err(err),
            },
        }
    }

    fn abort_merge(&self) -> AppResult<()> {
        self.run(&["merge", "--abort"])?;
        Ok(())
    }
}

impl Rebaser for Backend {
    fn rebase(&self, onto: &str, opts: Option<&RebaseOptions>) -> AppResult<RebaseResult> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["rebase".to_string()];
        if opts.interactive {
            args.push("-i".to_string());
        }
        if opts.autosquash {
            args.push("--autosquash".to_string());
        }
        args.extend(opts.extra_args);
        args.push(onto.to_string());

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let old_head = self.rev_parse("HEAD").ok();
        match self.run(&refs) {
            Ok(_) => {
                let new_head = self.rev_parse("HEAD")?;
                let applied = match &old_head {
                    Some(oh) => {
                        let range = format!("{}..{}", oh, new_head);
                        self.run(&["rev-list", "--count", &range])
                            .ok()
                            .and_then(|out| String::from_utf8_lossy(&out).trim().parse().ok())
                            .unwrap_or(0)
                    }
                    None => 0,
                };
                Ok(RebaseResult {
                    head: Some(new_head),
                    applied,
                    conflicts: Vec::new(),
                })
            }
            Err(err) => match conflict_stderr(&err) {
                Some(stderr) if stderr.contains("CONFLICT") => Ok(RebaseResult {
                    head: None,
                    applied: 0,
                    conflicts: parse_conflict_paths(self),
                }),
                _ => Err(err),
            },
        }
    }

    fn abort_rebase(&self) -> AppResult<()> {
        self.run(&["rebase", "--abort"])?;
        Ok(())
    }

    fn continue_rebase(&self) -> AppResult<RebaseResult> {
        let old_head = self.rev_parse("HEAD").ok();
        match self.run(&["rebase", "--continue"]) {
            Ok(_) => {
                let new_head = self.rev_parse("HEAD")?;
                let applied = match &old_head {
                    Some(old) => self
                        .run(&["rev-list", "--count", &format!("{old}..{new_head}")])
                        .ok()
                        .and_then(|out| String::from_utf8_lossy(&out).trim().parse::<usize>().ok())
                        .unwrap_or(0),
                    None => 0,
                };
                Ok(RebaseResult {
                    head: Some(new_head),
                    applied,
                    conflicts: Vec::new(),
                })
            }
            Err(err) => match conflict_stderr(&err) {
                Some(stderr) if stderr.contains("CONFLICT") => Ok(RebaseResult {
                    head: None,
                    applied: 0,
                    conflicts: parse_conflict_paths(self),
                }),
                _ => Err(err),
            },
        }
    }
}

impl CherryPicker for Backend {
    fn cherry_pick(&self, commit: &str, opts: Option<&CherryPickOptions>) -> AppResult<Oid> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["cherry-pick".to_string()];
        if let Some(mainline) = opts.mainline {
            args.push("-m".to_string());
            args.push(mainline.to_string());
        }
        if opts.no_commit {
            args.push("--no-commit".to_string());
        }
        args.extend(opts.extra_args);
        args.push(commit.to_string());

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&refs)?;
        self.rev_parse("HEAD")
    }

    fn cherry_pick_continue(&self) -> AppResult<Oid> {
        self.run(&["cherry-pick", "--continue"])?;
        self.rev_parse("HEAD")
    }

    fn cherry_pick_abort(&self) -> AppResult<()> {
        self.run(&["cherry-pick", "--abort"])?;
        Ok(())
    }
}

impl Resetter for Backend {
    fn reset(&self, target: &str, mode: ResetMode) -> AppResult<()> {
        let mode = match mode {
            ResetMode::Mixed => "--mixed",
            ResetMode::Soft => "--soft",
            ResetMode::Hard => "--hard",
        };
        self.run(&["reset", mode, target])?;
        Ok(())
    }
}

impl CheckoutManager for Backend {
    fn checkout(&self, ref_name: &str, opts: Option<&CheckoutOptions>) -> AppResult<()> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["checkout".to_string()];
        if opts.force {
            args.push("-f".to_string());
        }
        if let Some(branch) = opts.create_branch {
            args.push("-b".to_string());
            args.push(branch);
        }
        if opts.detach {
            args.push("--detach".to_string());
        }
        args.extend(opts.extra_args);
        args.push(ref_name.to_string());

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&refs)?;
        Ok(())
    }

    fn checkout_files(&self, paths: &[&str]) -> AppResult<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let mut args = vec!["checkout"];
        args.push("--");
        args.extend(paths.iter().copied());
        self.run(&args)?;
        Ok(())
    }
}

impl Stasher for Backend {
    fn stash(&self, message: &str) -> AppResult<Oid> {
        self.run(&["stash", "push", "-m", message])?;
        self.rev_parse("stash@{0}")
    }

    fn stash_pop(&self) -> AppResult<()> {
        self.run(&["stash", "pop"])?;
        Ok(())
    }

    fn stash_pop_index(&self, index: usize) -> AppResult<()> {
        let reference = format!("stash@{{{index}}}");
        self.run(&["stash", "pop", &reference])?;
        Ok(())
    }

    fn stash_list(&self) -> AppResult<Vec<StashEntry>> {
        let output = self.run(&["stash", "list"])?;
        String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| parse_stash_entry(self, line))
            .collect()
    }
}

fn parse_stash_entry(backend: &Backend, line: &str) -> AppResult<StashEntry> {
    let (reference, message) = line
        .split_once(": ")
        .ok_or_else(|| AppError::invalid_format("stash entry", line))?;
    let start = reference
        .find('{')
        .ok_or_else(|| AppError::invalid_format("stash entry", line))?;
    let end = reference[start + 1..]
        .find('}')
        .map(|idx| idx + start + 1)
        .ok_or_else(|| AppError::invalid_format("stash entry", line))?;
    let index = reference[start + 1..end]
        .parse::<usize>()
        .map_err(|_| AppError::invalid_format("stash index", line))?;

    Ok(StashEntry {
        index,
        oid: backend.rev_parse(reference)?,
        message: message.to_string(),
    })
}

fn conflict_stderr(error: &AppError) -> Option<String> {
    error
        .cause
        .as_ref()
        .map(|cause| cause.to_string())
        .or_else(|| Some(error.message.clone()))
}

fn parse_conflict_paths(backend: &Backend) -> Vec<String> {
    // Query git directly for conflicted paths rather than parsing human-readable messages.
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(backend.root())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let mut paths: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            paths.sort();
            paths.dedup();
            paths
        }
        _ => Vec::new(),
    }
}

//! Management operations for the CLI backend.

use std::time::{Duration, UNIX_EPOCH};

use rskit_errors::{AppError, AppResult};

use crate::error::GitError;
use crate::manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
use crate::options::{CleanOptions, FetchOptions, PushOptions};
use crate::types::{Branch, BranchFilter, Remote, Signature, Tag};

use super::Backend;

impl RefManager for Backend {
    fn list_branches(&self, filter: BranchFilter) -> AppResult<Vec<Branch>> {
        let mut args = vec![
            "for-each-ref".to_string(),
            "--format=%(refname:short)%00%(objectname)%00%(upstream:short)".to_string(),
        ];
        match filter {
            BranchFilter::Local => args.push("refs/heads".to_string()),
            BranchFilter::Remote => args.push("refs/remotes".to_string()),
            BranchFilter::All => {
                args.push("refs/heads".to_string());
                args.push("refs/remotes".to_string());
            }
        }

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = self.run(&refs)?;
        String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_branch)
            .collect::<AppResult<Vec<_>>>()
    }

    fn list_tags(&self) -> AppResult<Vec<Tag>> {
        let output = self.run(&[
            "for-each-ref",
            "-z",
            "refs/tags",
            "--format=%(refname:short)%00%(objecttype)%00%(objectname)%00%(*objectname)%00%(taggername)%00%(taggeremail)%00%(taggerdate:unix)%00%(contents)",
        ])?;
        parse_tags(&output)
    }

    fn create_branch(&self, name: &str, target: &str) -> AppResult<()> {
        self.run(&["branch", "--", name, target])?;
        Ok(())
    }

    fn delete_branch(&self, name: &str) -> AppResult<()> {
        let result = self.run(&["branch", "-d", "--", name]);
        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("not found") || msg.contains("no such branch") {
                    Err(GitError::RefNotFound {
                        refname: name.to_string(),
                    }
                    .into())
                } else {
                    Err(err)
                }
            }
        }
    }

    fn create_tag(&self, name: &str, target: &str, message: &str) -> AppResult<()> {
        if message.is_empty() {
            self.run(&["tag", "--", name, target])?;
        } else {
            self.run(&["tag", "-a", "-m", message, "--", name, target])?;
        }
        Ok(())
    }

    fn delete_tag(&self, name: &str) -> AppResult<()> {
        let result = self.run(&["tag", "-d", "--", name]);
        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("not found") || msg.contains("No such tag") {
                    Err(GitError::RefNotFound {
                        refname: name.to_string(),
                    }
                    .into())
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl RemoteManager for Backend {
    fn list_remotes(&self) -> AppResult<Vec<Remote>> {
        let output = self.run(&["remote", "-v"])?;
        let text = String::from_utf8_lossy(&output);
        let mut remotes = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let Some((name, rest)) = line.split_once('\t') else {
                continue;
            };
            if !seen.insert(name.to_string()) {
                continue;
            }
            let url = rest.rsplit_once(" (").map_or(rest, |(u, _)| u).to_string();
            remotes.push(Remote {
                name: name.to_string(),
                url,
                fetch_specs: self.config_get_all(&format!("remote.{name}.fetch"))?,
                push_specs: self.config_get_all(&format!("remote.{name}.push"))?,
            });
        }

        Ok(remotes)
    }

    fn fetch(&self, remote: &str, opts: Option<&FetchOptions>) -> AppResult<()> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["fetch".to_string()];
        if opts.prune {
            args.push("--prune".to_string());
        }
        if let Some(depth) = opts.depth {
            args.push("--depth".to_string());
            args.push(depth.to_string());
        }
        args.extend(opts.extra_args);
        args.push(remote.to_string());
        args.extend(opts.refspecs);

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&refs)?;
        Ok(())
    }

    fn push(&self, remote: &str, opts: Option<&PushOptions>) -> AppResult<()> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["push".to_string()];
        if opts.force {
            args.push("--force".to_string());
        }
        args.extend(opts.extra_args);
        args.push(remote.to_string());
        args.extend(opts.refspecs);

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&refs)?;
        Ok(())
    }

    fn tracking_branch(&self, branch: &str) -> AppResult<String> {
        let remote = self.config_get(&format!("branch.{branch}.remote"))?;
        let merge = self.config_get(&format!("branch.{branch}.merge"))?;
        let short = merge.trim_start_matches("refs/heads/");
        Ok(format!("{remote}/{short}"))
    }
}

impl ConfigReader for Backend {
    fn config_get(&self, key: &str) -> AppResult<String> {
        let args = ["config", "--get", "--", key];
        let output = self.run_result(&args)?;

        if output.success() && !output.stdout_truncated && !output.stderr_truncated {
            Ok(output.stdout.trim().to_string())
        } else if output.exit_code == Some(1) {
            Err(GitError::ConfigNotFound {
                key: key.to_string(),
            }
            .into())
        } else {
            Err(Backend::command_failed(&args, output))
        }
    }

    fn config_get_all(&self, key: &str) -> AppResult<Vec<String>> {
        let output = run_allow_empty(self, &["config", "--get-all", key])?;
        Ok(String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect())
    }

    fn config_set(&self, key: &str, value: &str) -> AppResult<()> {
        self.run(&["config", key, value])?;
        Ok(())
    }
}

impl Maintainer for Backend {
    fn gc(&self) -> AppResult<()> {
        self.run(&["gc"])?;
        Ok(())
    }

    fn prune(&self) -> AppResult<()> {
        self.run(&["prune"])?;
        Ok(())
    }

    fn fsck(&self) -> AppResult<()> {
        self.run(&["fsck"])?;
        Ok(())
    }

    fn clean(&self, opts: Option<&CleanOptions>) -> AppResult<Vec<String>> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["clean".to_string()];
        if opts.directories {
            args.push("-d".to_string());
        }
        if opts.ignored {
            args.push("-x".to_string());
        }
        if opts.force {
            args.push("-f".to_string());
        } else {
            args.push("-n".to_string());
        }
        args.extend(opts.extra_args);

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = self.run(&refs)?;
        Ok(String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.strip_prefix("Removing ")
                    .or_else(|| line.strip_prefix("Would remove "))
                    .map(str::to_string)
            })
            .collect())
    }
}

fn run_allow_empty(backend: &Backend, args: &[&str]) -> AppResult<Vec<u8>> {
    let output = backend.run_result(args)?;
    if (output.success() || output.exit_code == Some(1))
        && !output.stdout_truncated
        && !output.stderr_truncated
    {
        Ok(output.stdout_bytes)
    } else {
        Err(Backend::command_failed(args, output))
    }
}

fn parse_branch(line: &str) -> AppResult<Branch> {
    let mut parts = line.split('\0');
    let name = parts.next().unwrap_or_default().to_string();
    let target = super::parse_oid(parts.next().unwrap_or_default())?;
    let upstream = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(Branch {
        name,
        target,
        upstream,
    })
}

fn parse_tags(output: &[u8]) -> AppResult<Vec<Tag>> {
    let text = String::from_utf8_lossy(output);
    let fields = text.split_terminator('\0').collect::<Vec<_>>();
    let chunks = fields.chunks_exact(8);
    if !chunks.remainder().is_empty() {
        return Err(AppError::invalid_format(
            "tag list",
            "records with 8 NUL-separated fields",
        ));
    }

    chunks.map(parse_tag).collect()
}

fn parse_tag(fields: &[&str]) -> AppResult<Tag> {
    let &[
        name,
        object_type,
        object_oid,
        peeled_oid,
        tagger_name,
        tagger_email,
        tagger_date,
        message,
    ] = fields
    else {
        return Err(AppError::invalid_format("tag", "8 NUL-separated fields"));
    };
    let target = if object_type == "tag" && !peeled_oid.is_empty() {
        super::parse_oid(peeled_oid)?
    } else {
        super::parse_oid(object_oid)?
    };
    let tagger = if !tagger_name.is_empty() {
        let when = tagger_date
            .trim()
            .parse::<i64>()
            .ok()
            .and_then(|secs| {
                if secs >= 0 {
                    UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
                } else {
                    UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
                }
            })
            .unwrap_or(UNIX_EPOCH);
        Some(Signature {
            name: (*tagger_name).to_string(),
            email: tagger_email
                .trim_matches(|c| c == '<' || c == '>')
                .to_string(),
            when,
        })
    } else {
        None
    };
    Ok(Tag {
        name: (*name).to_string(),
        target,
        tagger,
        message: if object_type == "tag" {
            message.trim_end_matches('\n').to_string()
        } else {
            String::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::parse_tags;

    #[test]
    fn parse_tags_preserves_multiline_contents() {
        let output = concat!(
            "v0.2.0\0",
            "tag\0",
            "0000000000000000000000000000000000000000\0",
            "1111111111111111111111111111111111111111\0",
            "Test User\0",
            "test@example.com\0",
            "1700000000\0",
            "release\nnotes\0"
        )
        .as_bytes();
        let tags = parse_tags(output).unwrap();

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "v0.2.0");
        assert_eq!(tags[0].message, "release\nnotes");
        assert_eq!(
            tags[0].target.to_string(),
            "1111111111111111111111111111111111111111"
        );
    }
}

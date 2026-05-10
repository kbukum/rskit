//! Management trait stubs for the CLI backend.

use std::process::Command;

use rskit_errors::AppResult;

use crate::manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
use crate::options::{CleanOptions, FetchOptions, PushOptions};
use crate::types::{Branch, BranchFilter, Remote, Tag};

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
            "refs/tags",
            "--format=%(refname:short)%00%(objecttype)%00%(objectname)%00%(*objectname)%00%(taggername)%00%(taggeremail)%00%(contents)",
        ])?;
        String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_tag)
            .collect::<AppResult<Vec<_>>>()
    }

    fn create_branch(&self, name: &str, target: &str) -> AppResult<()> {
        self.run(&["branch", name, target])?;
        Ok(())
    }

    fn delete_branch(&self, name: &str) -> AppResult<()> {
        self.run(&["branch", "-d", name])?;
        Ok(())
    }

    fn create_tag(&self, name: &str, target: &str, message: &str) -> AppResult<()> {
        if message.is_empty() {
            self.run(&["tag", name, target])?;
        } else {
            self.run(&["tag", "-a", name, target, "-m", message])?;
        }
        Ok(())
    }

    fn delete_tag(&self, name: &str) -> AppResult<()> {
        self.run(&["tag", "-d", name])?;
        Ok(())
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
            let url = rest.split(' ').next().unwrap_or_default().to_string();
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
        let output = self.run(&["config", "--get", key])?;
        Ok(String::from_utf8_lossy(&output).trim().to_string())
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
    let output = Command::new("git")
        .args(args)
        .current_dir(backend.root())
        .output()?;
    if output.status.success() || output.status.code() == Some(1) {
        Ok(output.stdout)
    } else {
        backend.run(args)
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

fn parse_tag(line: &str) -> AppResult<Tag> {
    let mut parts = line.splitn(7, '\0');
    let name = parts.next().unwrap_or_default().to_string();
    let object_type = parts.next().unwrap_or_default();
    let object_oid = parts.next().unwrap_or_default();
    let peeled_oid = parts.next().unwrap_or_default();
    let _tagger_name = parts.next().unwrap_or_default();
    let _tagger_email = parts.next().unwrap_or_default();
    let message = parts.next().unwrap_or_default().trim().to_string();
    let target = if object_type == "tag" && !peeled_oid.is_empty() {
        super::parse_oid(peeled_oid)?
    } else {
        super::parse_oid(object_oid)?
    };
    Ok(Tag {
        name,
        target,
        tagger: None,
        message: if object_type == "tag" {
            message
        } else {
            String::new()
        },
    })
}

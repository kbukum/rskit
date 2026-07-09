//! VCS-ignore-aware tree walking.
//!
//! [`walk_tree_ignoring`] traverses a directory subtree while honouring
//! `.gitignore` / `.ignore` rules (and, by default, skipping the `.git`
//! directory and hidden entries), built on the canonical `ignore` crate. It is
//! the traversal to use when a consumer wants only version-controlled or
//! source-relevant files — content hashing, indexing, or packaging — rather
//! than the raw on-disk tree that [`super::walk_tree`] yields.

use std::ffi::OsStr;
use std::path::Path;

use ignore::WalkBuilder;
use rskit_errors::{AppError, AppResult, ErrorCode};

use super::{TreeEntry, WalkControl, ensure_directory};

/// Options controlling a VCS-ignore-aware walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoreWalkOptions {
    /// Honour versioned ignore files — `.gitignore`, `.ignore`, and nested or
    /// parent ignore files discovered along the walk. When `false` only the
    /// built-in and explicit skips apply. Per-machine, non-versioned sources
    /// (the global gitignore and `.git/info/exclude`) are never consulted, so
    /// results stay reproducible across developers and CI.
    pub respect_gitignore: bool,
    /// Skip dot-prefixed files and directories (e.g. `.git`, `.cache`).
    pub skip_hidden: bool,
    /// Follow symlinks instead of visiting them as link entries.
    pub follow_symlinks: bool,
}

impl Default for IgnoreWalkOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            skip_hidden: true,
            follow_symlinks: false,
        }
    }
}

/// Walk `root`, invoking `visitor` for every regular file that survives the
/// active ignore rules.
///
/// Directories and symlinks are never handed to the visitor; only regular
/// files are, mirroring the file-oriented digests and indexers that consume
/// this. Entries are yielded in the `ignore` crate's traversal order, so a
/// caller that needs a stable identity must sort by [`TreeEntry::relative_path`].
///
/// The visitor returns a [`WalkControl`], matching [`super::walk_tree`], so a
/// consumer can stop early with [`WalkControl::Stop`]. Because only leaf files
/// are visited, [`WalkControl::SkipSubtree`] has nothing to prune and behaves
/// like [`WalkControl::Continue`].
///
/// This helper uses blocking `std::fs` I/O. Run it through
/// `tokio::task::spawn_blocking` or an equivalent blocking boundary when
/// calling from async code.
pub fn walk_tree_ignoring(
    root: &Path,
    options: IgnoreWalkOptions,
    mut visitor: impl FnMut(&TreeEntry) -> AppResult<WalkControl>,
) -> AppResult<()> {
    ensure_directory(root, options.follow_symlinks)?;

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(options.skip_hidden)
        .parents(options.respect_gitignore)
        .ignore(options.respect_gitignore)
        .git_ignore(options.respect_gitignore)
        // The global gitignore (`core.excludesfile`) and `.git/info/exclude` are
        // per-machine, non-versioned state; excluding them keeps digests
        // reproducible regardless of who runs the walk.
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .follow_links(options.follow_symlinks);
    // `.git` metadata is never source content; drop it even when git ignore
    // handling is off so a digest cannot churn on repository state.
    builder.filter_entry(|entry| entry.file_name() != OsStr::new(".git"));

    for result in builder.build() {
        let entry = result.map_err(|error| walk_ignore_error(&error))?;
        let Some(file_type) = entry.file_type() else {
            // The root sentinel with no file type; nothing to hash.
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let relative_path = path.strip_prefix(root).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to strip prefix: {error}"),
            )
        })?;

        let tree_entry = TreeEntry {
            path: path.to_path_buf(),
            relative_path: relative_path.to_path_buf(),
            is_file: true,
            is_dir: false,
            is_symlink: false,
        };
        match visitor(&tree_entry)? {
            WalkControl::Stop => break,
            WalkControl::Continue | WalkControl::SkipSubtree => {}
        }
    }

    Ok(())
}

fn walk_ignore_error(error: &ignore::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to walk directory tree: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::{IgnoreWalkOptions, walk_tree_ignoring};
    use crate::TempDir;
    use crate::sync_io::tree::WalkControl;

    fn collect(root: &std::path::Path, options: IgnoreWalkOptions) -> BTreeSet<PathBuf> {
        let mut seen = BTreeSet::new();
        walk_tree_ignoring(root, options, |entry| {
            seen.insert(entry.relative_path.clone());
            Ok(WalkControl::Continue)
        })
        .unwrap();
        seen
    }

    #[test]
    fn skips_gitignored_paths() {
        let dir = TempDir::new().unwrap();
        dir.write_file(".gitignore", b"target/\n*.log\n").unwrap();
        dir.write_file("src/lib.rs", b"fn a() {}").unwrap();
        dir.write_file("target/debug/app", b"binary").unwrap();
        dir.write_file("run.log", b"noise").unwrap();

        let seen = collect(dir.path(), IgnoreWalkOptions::default());

        assert!(seen.contains(&PathBuf::from("src/lib.rs")));
        // The default skips hidden entries, so the dot-prefixed `.gitignore`
        // itself is not yielded.
        assert!(!seen.contains(&PathBuf::from(".gitignore")));
        assert!(!seen.iter().any(|p| p.starts_with("target")));
        assert!(!seen.contains(&PathBuf::from("run.log")));
    }

    #[test]
    fn keeps_dotfiles_but_still_drops_git_and_ignored() {
        // The digest use case: hidden config (`.cargo`, `.gitignore`) is source
        // and must be hashed, but `.git` and gitignored build output must not.
        let dir = TempDir::new().unwrap();
        dir.write_file(".gitignore", b"target/\n").unwrap();
        dir.write_file(".cargo/config.toml", b"[build]").unwrap();
        dir.write_file(".git/HEAD", b"ref: x").unwrap();
        dir.write_file("src/lib.rs", b"fn a() {}").unwrap();
        dir.write_file("target/debug/app", b"binary").unwrap();

        let options = IgnoreWalkOptions {
            skip_hidden: false,
            ..IgnoreWalkOptions::default()
        };
        let seen = collect(dir.path(), options);

        assert!(seen.contains(&PathBuf::from("src/lib.rs")));
        assert!(seen.contains(&PathBuf::from(".gitignore")));
        assert!(seen.contains(&PathBuf::from(".cargo/config.toml")));
        assert!(!seen.iter().any(|p| p.starts_with(".git/")));
        assert!(!seen.iter().any(|p| p.starts_with("target")));
    }

    #[test]
    fn skips_git_directory_even_without_ignore() {
        let dir = TempDir::new().unwrap();
        dir.write_file(".git/config", b"[core]").unwrap();
        dir.write_file("src/lib.rs", b"fn a() {}").unwrap();

        let options = IgnoreWalkOptions {
            respect_gitignore: false,
            skip_hidden: false,
            follow_symlinks: false,
        };
        let seen = collect(dir.path(), options);

        assert!(seen.contains(&PathBuf::from("src/lib.rs")));
        assert!(!seen.iter().any(|p| p.starts_with(".git")));
    }

    #[test]
    fn honours_nested_ignore_files() {
        let dir = TempDir::new().unwrap();
        dir.write_file("crate/.gitignore", b"generated/\n").unwrap();
        dir.write_file("crate/src/main.rs", b"fn main() {}")
            .unwrap();
        dir.write_file("crate/generated/out.rs", b"// gen").unwrap();

        let seen = collect(dir.path(), IgnoreWalkOptions::default());

        assert!(seen.contains(&PathBuf::from("crate/src/main.rs")));
        assert!(!seen.iter().any(|p| p.starts_with("crate/generated")));
    }

    #[test]
    fn ignores_non_versioned_git_exclude() {
        // `.git/info/exclude` is per-repo, non-versioned state; honouring it
        // would make digests depend on local machine state, so it must not
        // filter the walk.
        let dir = TempDir::new().unwrap();
        dir.write_file(".git/info/exclude", b"secret.rs\n").unwrap();
        dir.write_file("secret.rs", b"fn s() {}").unwrap();

        let seen = collect(dir.path(), IgnoreWalkOptions::default());

        assert!(seen.contains(&PathBuf::from("secret.rs")));
    }

    #[test]
    fn stops_early_on_walk_control_stop() {
        let dir = TempDir::new().unwrap();
        for index in 0..5 {
            dir.write_file(format!("f{index}.txt"), b"x").unwrap();
        }

        let mut visited = 0usize;
        walk_tree_ignoring(dir.path(), IgnoreWalkOptions::default(), |_| {
            visited += 1;
            Ok(WalkControl::Stop)
        })
        .unwrap();

        assert_eq!(visited, 1);
    }

    #[test]
    fn rejects_non_directory_root() {
        let dir = TempDir::new().unwrap();
        let file = dir.write_file("file.txt", b"hi").unwrap();
        let result = walk_tree_ignoring(&file, IgnoreWalkOptions::default(), |_| {
            Ok(WalkControl::Continue)
        });
        assert!(result.is_err());
    }
}

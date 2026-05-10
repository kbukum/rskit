use std::fs;
use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};
use tempfile::TempDir;

use crate::{Checker, Committer, ConfigReader, Differ, IndexManager, RefManager, Repo};

/// Builder for creating test repositories with specific states.
pub struct RepoBuilder {
    _tempdir: TempDir,
    root: PathBuf,
    repo: Repo,
}

impl RepoBuilder {
    /// Creates a new test repository in a temporary directory.
    pub fn new() -> AppResult<Self> {
        let tempdir = TempDir::new().map_err(AppError::internal)?;
        let root = tempdir.path().to_path_buf();
        let repo = crate::init(&root)?;
        repo.config_set("user.name", "Test User")?;
        repo.config_set("user.email", "test@example.com")?;
        Ok(Self {
            _tempdir: tempdir,
            root,
            repo,
        })
    }

    /// Creates or overwrites a file in the working tree.
    #[must_use = "builder methods return the updated builder; chain or bind the result"]
    pub fn with_file(self, path: &str, content: &str) -> AppResult<Self> {
        let full_path = self.root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        fs::write(full_path, content).map_err(AppError::internal)?;
        Ok(self)
    }

    /// Stages all changes and creates a commit.
    #[must_use = "builder methods return the updated builder; chain or bind the result"]
    pub fn with_commit(self, message: &str) -> AppResult<Self> {
        let paths = self
            .repo
            .status()?
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        let refs = paths.iter().map(String::as_str).collect::<Vec<_>>();
        if !refs.is_empty() {
            self.repo.stage(&refs)?;
        }
        self.repo.commit(message, None)?;
        Ok(self)
    }

    /// Creates a new branch at HEAD.
    #[must_use = "builder methods return the updated builder; chain or bind the result"]
    pub fn with_branch(self, name: &str) -> AppResult<Self> {
        self.repo.create_branch(name, "HEAD")?;
        Ok(self)
    }

    /// Switches to the named branch.
    #[must_use = "builder methods return the updated builder; chain or bind the result"]
    pub fn with_checkout(self, branch: &str) -> AppResult<Self> {
        self.repo.checkout(branch, None)?;
        Ok(self)
    }

    /// Creates a tag at HEAD.
    #[must_use = "builder methods return the updated builder; chain or bind the result"]
    pub fn with_tag(self, name: &str, message: &str) -> AppResult<Self> {
        self.repo.create_tag(name, "HEAD", message)?;
        Ok(self)
    }

    /// Returns a reference to the repository.
    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    /// Returns the repository root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use crate::{BranchFilter, LogOptions, LogReader, RefManager, Repository};

    use super::RepoBuilder;

    #[test]
    fn repo_builder_creates_commits_and_refs() {
        let builder = RepoBuilder::new()
            .unwrap()
            .with_file("README.md", "# repo\n")
            .unwrap()
            .with_commit("initial commit")
            .unwrap()
            .with_branch("feature")
            .unwrap()
            .with_tag("v1.0.0", "release")
            .unwrap()
            .with_checkout("feature")
            .unwrap();

        assert_eq!(
            builder.repo().root().canonicalize().unwrap(),
            builder.root().canonicalize().unwrap()
        );

        let commits = builder
            .repo()
            .log(Some(&LogOptions {
                max_count: Some(1),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(commits[0].message, "initial commit");

        let branches = builder.repo().list_branches(BranchFilter::Local).unwrap();
        assert!(branches.iter().any(|branch| branch.name == "feature"));
        let tags = builder.repo().list_tags().unwrap();
        assert!(tags.iter().any(|tag| tag.name == "v1.0.0"));
    }

    #[test]
    fn repo_builder_writes_files() {
        let builder = RepoBuilder::new()
            .unwrap()
            .with_file("nested/file.txt", "hello\n")
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(builder.root().join("nested/file.txt")).unwrap(),
            "hello\n"
        );
    }
}

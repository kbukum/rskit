//! Git repository path helpers.

use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult};

use crate::error::GitError;

/// Return `path` relative to `repo_root` after canonicalizing both paths.
pub fn repo_relative_path(repo_root: &Path, path: &Path) -> AppResult<PathBuf> {
    let repo_root = repo_root.canonicalize().map_err(|error| {
        AppError::invalid_input(
            "repo_root",
            format!("failed to resolve git root '{}'", repo_root.display()),
        )
        .with_cause(error)
    })?;
    let path = path.canonicalize().map_err(|error| {
        AppError::invalid_input(
            "path",
            format!("failed to resolve path '{}'", path.display()),
        )
        .with_cause(error)
    })?;
    path.strip_prefix(&repo_root)
        .map(normalize_path)
        .map_err(|error| {
            AppError::invalid_input(
                "path",
                format!(
                    "path '{}' is not inside git root '{}'",
                    path.display(),
                    repo_root.display()
                ),
            )
            .with_cause(error)
        })
}

/// Join a repository-relative prefix and path, preserving empty prefixes.
pub fn join_repo_path(prefix: &Path, relative: &Path) -> AppResult<PathBuf> {
    validate_repo_relative("prefix", prefix)?;
    validate_repo_relative("path", relative)?;
    if prefix.as_os_str().is_empty() {
        Ok(normalize_path(relative))
    } else {
        Ok(normalize_path(&prefix.join(relative)))
    }
}

pub(crate) fn validate_repo_relative_path(path: &str) -> AppResult<()> {
    rskit_fs::validate_relative_path(Path::new(path)).map_err(|error| {
        AppError::from(GitError::InvalidPath {
            path: path.to_string(),
        })
        .with_cause(error)
    })
}

fn validate_repo_relative(field: &'static str, path: &Path) -> AppResult<()> {
    rskit_fs::validate_relative_path(path).map_err(|error| {
        AppError::invalid_input(
            field,
            format!("repository path {}: {error}", path.display()),
        )
        .with_cause(error)
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(PathBuf::from(value)),
            Component::CurDir => None,
            _ => Some(PathBuf::from(component.as_os_str())),
        })
        .fold(PathBuf::new(), |mut normalized, component| {
            normalized.push(component);
            normalized
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{join_repo_path, repo_relative_path};

    #[test]
    fn returns_path_relative_to_repo_root() {
        let root = rskit_testutil::test_workspace!("git-repo-relative");
        let repo = root.path().join("repo");
        let workspace = repo.join("apps/web");
        fs::create_dir_all(&workspace).expect("create workspace");

        let relative = repo_relative_path(&repo, &workspace).expect("path is inside repo");

        assert_eq!(relative, std::path::Path::new("apps/web"));
    }

    #[test]
    fn joins_empty_prefix_as_relative_path() {
        assert_eq!(
            join_repo_path(std::path::Path::new(""), std::path::Path::new("src/lib.rs"))
                .expect("path is safe"),
            std::path::Path::new("src/lib.rs")
        );
    }

    #[test]
    fn rejects_absolute_relative_path() {
        let error = join_repo_path(std::path::Path::new("repo"), std::path::Path::new("/tmp/x"))
            .expect_err("absolute paths escape the repository prefix");

        assert!(error.message().contains("path must be relative"));
    }

    #[test]
    fn rejects_parent_traversal_path() {
        let error = join_repo_path(std::path::Path::new("repo"), std::path::Path::new("../x"))
            .expect_err("parent traversal escapes the repository prefix");

        assert!(error.message().contains("must not contain '..'"));
    }

    #[test]
    fn rejects_escaping_prefix() {
        let error = join_repo_path(std::path::Path::new("../repo"), std::path::Path::new("x"))
            .expect_err("prefix must also be repository relative");

        assert!(error.message().contains("must not contain '..'"));
    }
}

//! Git repository path helpers.

use std::path::{Component, Path, PathBuf};

use rskit_errors::{AppError, AppResult};

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
#[must_use]
pub fn join_repo_path(prefix: &Path, relative: &Path) -> PathBuf {
    if prefix.as_os_str().is_empty() {
        relative.to_path_buf()
    } else {
        prefix.join(relative)
    }
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
            join_repo_path(std::path::Path::new(""), std::path::Path::new("src/lib.rs")),
            std::path::Path::new("src/lib.rs")
        );
    }
}

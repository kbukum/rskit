//! Temporary workspace and fixture helpers for tests.

use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};
use rskit_fs::{TempDir, safe_join, sync_io::file};

/// Managed temporary workspace for unit and integration tests.
///
/// The workspace is deleted when dropped. Fixture operations are rooted at an
/// explicit fixture directory and reject absolute or parent-traversing relative
/// paths through `rskit-fs` safe path handling.
#[derive(Debug)]
pub struct TestWorkspace {
    inner: TempDir,
    fixture_dir: Option<PathBuf>,
}

impl TestWorkspace {
    /// Create a temporary workspace, panicking with `label` on failure.
    pub fn new(label: &str) -> Self {
        Self::try_new().unwrap_or_else(|error| panic!("create test workspace for {label}: {error}"))
    }

    /// Try to create a temporary workspace.
    pub fn try_new() -> AppResult<Self> {
        Ok(Self {
            inner: TempDir::new()?,
            fixture_dir: None,
        })
    }

    /// Set the directory used as the root for fixture lookups.
    #[must_use]
    pub fn with_fixture_dir(mut self, fixture_dir: impl Into<PathBuf>) -> Self {
        self.fixture_dir = Some(fixture_dir.into());
        self
    }

    /// Return the workspace root directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Resolve a safe relative path inside the workspace.
    pub fn child(&self, rel_path: impl AsRef<Path>) -> AppResult<PathBuf> {
        self.inner.child(rel_path)
    }

    /// Write a file inside the workspace, creating parent directories.
    pub fn write_file(&self, rel_path: impl AsRef<Path>, content: &[u8]) -> AppResult<PathBuf> {
        self.inner.write_file(rel_path, content)
    }

    /// Resolve a fixture path under the configured fixture root.
    pub fn fixture_path(&self, rel_path: impl AsRef<Path>) -> AppResult<PathBuf> {
        let Some(fixture_dir) = &self.fixture_dir else {
            return Err(AppError::invalid_input(
                "fixture_dir",
                "fixture directory is not configured",
            ));
        };
        safe_join(fixture_dir, rel_path.as_ref())
            .map_err(|error| AppError::invalid_input("fixture path", error.to_string()))
    }

    /// Read fixture bytes from the configured fixture root.
    pub fn read_fixture(&self, rel_path: impl AsRef<Path>) -> AppResult<Vec<u8>> {
        file::read(&self.fixture_path(rel_path)?)
    }

    /// Read a UTF-8 fixture from the configured fixture root.
    pub fn read_fixture_string(&self, rel_path: impl AsRef<Path>) -> AppResult<String> {
        file::read_string(&self.fixture_path(rel_path)?)
    }

    /// Copy a fixture into the workspace, creating destination parent directories.
    pub fn copy_fixture(
        &self,
        fixture_rel_path: impl AsRef<Path>,
        dest_rel_path: impl AsRef<Path>,
    ) -> AppResult<PathBuf> {
        let source = self.fixture_path(fixture_rel_path)?;
        let dest = self.child(dest_rel_path)?;
        file::copy(&source, &dest)?;
        Ok(dest)
    }
}

/// Create a [`TestWorkspace`] whose fixture root is `<caller>/tests/fixtures`.
///
/// The caller manifest directory is captured at macro expansion time, so this
/// can be used from any crate that depends on `rskit-testutil`.
#[macro_export]
macro_rules! test_workspace {
    ($label:expr) => {
        $crate::TestWorkspace::new($label).with_fixture_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures"),
        )
    };
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rskit_errors::ErrorCode;

    use super::TestWorkspace;

    #[test]
    fn writes_files_under_workspace() {
        let workspace = TestWorkspace::new("write");

        let path = workspace.write_file("nested/file.txt", b"hello").unwrap();

        assert!(path.starts_with(workspace.path()));
        assert_eq!(
            rskit_fs::sync_io::file::read_string(&path).unwrap(),
            "hello"
        );
    }

    #[test]
    fn copies_fixture_into_workspace() {
        let fixtures = TestWorkspace::new("fixtures");
        fixtures
            .write_file("config/app.toml", b"name = 'demo'")
            .unwrap();
        let workspace = TestWorkspace::new("workspace").with_fixture_dir(fixtures.path());

        let copied = workspace
            .copy_fixture("config/app.toml", "copied/app.toml")
            .unwrap();

        assert!(copied.starts_with(workspace.path()));
        assert_eq!(
            rskit_fs::sync_io::file::read_string(&copied).unwrap(),
            "name = 'demo'"
        );
    }

    #[test]
    fn reads_fixture_string() {
        let fixtures = TestWorkspace::new("fixtures");
        fixtures.write_file("message.txt", b"hello").unwrap();
        let workspace = TestWorkspace::new("workspace").with_fixture_dir(fixtures.path());

        let content = workspace.read_fixture_string("message.txt").unwrap();

        assert_eq!(content, "hello");
    }

    #[test]
    fn rejects_fixture_access_without_fixture_dir() {
        let workspace = TestWorkspace::new("workspace");

        let error = workspace.fixture_path("message.txt").unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(
            error
                .message
                .contains("fixture directory is not configured")
        );
    }

    #[test]
    fn rejects_escaping_fixture_paths() {
        let fixtures = TestWorkspace::new("fixtures");
        let workspace = TestWorkspace::new("workspace").with_fixture_dir(fixtures.path());

        let error = workspace
            .fixture_path(Path::new("../escape.txt"))
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
}

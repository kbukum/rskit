use std::path::{Path, PathBuf};

use rskit_errors::{AppError, AppResult};
use rskit_fs::sync_io::file;
use rskit_util::env;

use super::matcher::Match;

/// Environment variable that switches golden verification into bless mode.
///
/// When set to any non-empty value, [`Golden::verify`] regenerates the expected
/// file from the (normalized) actual output instead of comparing — the one knob
/// callers document for "update the goldens".
pub const BLESS_ENV: &str = "RSKIT_BLESS";

/// Whether a golden run compares against the expected file or regenerates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenMode {
    /// Compare actual output against the stored golden.
    Verify,
    /// Overwrite the stored golden with the (normalized) actual output.
    Bless,
}

impl GoldenMode {
    /// The mode requested by the environment: [`GoldenMode::Bless`] when
    /// [`BLESS_ENV`] is set to a non-empty value, [`GoldenMode::Verify`] otherwise.
    #[must_use]
    pub fn from_env() -> Self {
        if env::get_non_empty(BLESS_ENV).is_some() {
            Self::Bless
        } else {
            Self::Verify
        }
    }
}

/// What a successful golden run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenOutcome {
    /// The actual output matched the stored golden.
    Verified,
    /// The stored golden was regenerated from the actual output.
    Blessed,
}

/// A handle over one on-disk expected file and the matcher that judges it.
#[derive(Debug, Clone)]
pub struct Golden {
    path: PathBuf,
    matcher: Match,
}

impl Golden {
    /// A golden at `path`, compared under `matcher`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, matcher: Match) -> Self {
        Self {
            path: path.into(),
            matcher,
        }
    }

    /// The expected file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Compare `actual` against the stored golden, in the mode requested by the
    /// environment ([`GoldenMode::from_env`]).
    ///
    /// # Errors
    ///
    /// See [`Golden::run`].
    pub fn verify(&self, actual: &str) -> AppResult<GoldenOutcome> {
        self.run(actual, GoldenMode::from_env())
    }

    /// Compare `actual` against the stored golden ([`GoldenMode::Verify`]), or
    /// regenerate the golden from it ([`GoldenMode::Bless`]).
    ///
    /// # Errors
    ///
    /// In verify mode, returns a typed [`AppError`]: a `NotFound`-class error
    /// when the golden file is missing (never a silent pass), or the matcher's
    /// mismatch diff. In bless mode, returns the underlying write error.
    pub fn run(&self, actual: &str, mode: GoldenMode) -> AppResult<GoldenOutcome> {
        match mode {
            GoldenMode::Bless => {
                file::create_parent_dir(&self.path)?;
                file::write(&self.path, self.matcher.normalize(actual))?;
                Ok(GoldenOutcome::Blessed)
            }
            GoldenMode::Verify => {
                if !file::exists(&self.path)? {
                    return Err(AppError::not_found(
                        "golden file",
                        Some(&self.path.display().to_string()),
                    )
                    .hint(format!("set {BLESS_ENV}=1 to generate it from live output")));
                }
                let expected = file::read_string(&self.path)?;
                self.matcher
                    .verify(&expected, actual)
                    .map_err(|err| err.with_detail("golden", self.path.display().to_string()))?;
                Ok(GoldenOutcome::Verified)
            }
        }
    }
}

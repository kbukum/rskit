//! Process-current-directory guard for tests.
//!
//! The process working directory is global mutable state: a test that calls
//! [`std::env::set_current_dir`] mutates a value every other test in the same
//! binary observes. [`CurrentDirGuard`] makes that safe by
//!
//! 1. holding a process-wide lock for its lifetime, so concurrent tests in the
//!    same binary cannot interleave their directory changes, and
//! 2. restoring the previous working directory on drop, so a change never leaks
//!    into a later test.
//!
//! It is the working-directory analogue of the env-var mutex pattern: any test
//! that depends on or mutates the current directory should hold a guard for the
//! duration of that dependence.
//!
//! ```no_run
//! use rskit_testutil::CurrentDirGuard;
//!
//! # fn example(repo_root: &std::path::Path) -> rskit_errors::AppResult<()> {
//! let _cwd = CurrentDirGuard::change_to(repo_root)?;
//! // ... code here observes `repo_root` as the working directory ...
//! // the previous directory is restored when `_cwd` drops.
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};

use parking_lot::{ReentrantMutex, ReentrantMutexGuard};
use rskit_errors::{AppError, AppResult};

/// Serializes every [`CurrentDirGuard`] across threads in the test binary so
/// concurrent tests cannot interleave process-wide working-directory changes.
///
/// The lock is reentrant: a single thread may hold nested guards (for example a
/// helper that calls [`CurrentDirGuard::pin`] inside a test that already holds a
/// guard) without deadlocking, while guards on *other* threads still block until
/// every guard on the holding thread is dropped.
static CWD_LOCK: ReentrantMutex<()> = ReentrantMutex::new(());

/// An RAII guard that pins the process working directory for its lifetime and
/// restores the previous directory when dropped.
///
/// Construct one with [`CurrentDirGuard::change_to`] (change the directory) or
/// [`CurrentDirGuard::pin`] (hold the current directory without changing it).
/// While the guard is alive it owns a process-wide reentrant lock, so guards on
/// other threads block until it drops; nested guards on the same thread are
/// allowed.
#[derive(Debug)]
#[must_use = "the working directory is restored when the guard is dropped; bind it to a name"]
pub struct CurrentDirGuard {
    previous: PathBuf,
    _lock: ReentrantMutexGuard<'static, ()>,
}

impl CurrentDirGuard {
    /// Acquire the lock and pin the current working directory without changing
    /// it, restoring it on drop.
    ///
    /// # Errors
    /// Returns an error if the current working directory cannot be read.
    pub fn pin() -> AppResult<Self> {
        let lock = CWD_LOCK.lock();
        let previous = current_dir()?;
        Ok(Self {
            previous,
            _lock: lock,
        })
    }

    /// Acquire the lock, record the current directory, and switch to `dir`.
    ///
    /// The previous directory is restored when the guard drops.
    ///
    /// # Errors
    /// Returns an error if the current directory cannot be read or if `dir`
    /// cannot be set as the working directory.
    pub fn change_to(dir: impl AsRef<Path>) -> AppResult<Self> {
        let lock = CWD_LOCK.lock();
        let previous = current_dir()?;
        set_current_dir(dir.as_ref())?;
        Ok(Self {
            previous,
            _lock: lock,
        })
    }

    /// The working directory captured when the guard was created.
    #[must_use]
    pub fn previous(&self) -> &Path {
        &self.previous
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        // Best-effort restore: a failure here cannot be surfaced from `drop`,
        // but the held lock means no other guard runs until this one releases.
        let _ = std::env::set_current_dir(&self.previous);
    }
}

fn current_dir() -> AppResult<PathBuf> {
    std::env::current_dir()
        .map_err(|error| AppError::from(error).context("failed to read current directory"))
}

fn set_current_dir(dir: &Path) -> AppResult<()> {
    std::env::set_current_dir(dir).map_err(|error| {
        AppError::from(error).context(format!(
            "failed to set current directory to '{}'",
            dir.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::CurrentDirGuard;
    use crate::TestWorkspace;

    #[test]
    fn change_to_switches_and_restores_on_drop() {
        let before = std::env::current_dir().unwrap();
        let workspace = TestWorkspace::new("cwd-guard");
        // Canonicalize: macOS temp dirs resolve through a `/private` symlink.
        let target = workspace.path().canonicalize().unwrap();

        {
            let guard = CurrentDirGuard::change_to(&target).unwrap();
            assert_eq!(std::env::current_dir().unwrap(), target);
            assert_eq!(guard.previous(), before);
        }

        assert_eq!(std::env::current_dir().unwrap(), before);
    }

    #[test]
    fn pin_holds_the_current_directory() {
        let before = std::env::current_dir().unwrap();

        {
            let _guard = CurrentDirGuard::pin().unwrap();
            assert_eq!(std::env::current_dir().unwrap(), before);
        }

        assert_eq!(std::env::current_dir().unwrap(), before);
    }

    #[test]
    fn change_to_rejects_a_missing_directory() {
        let workspace = TestWorkspace::new("cwd-missing");
        let missing = workspace.path().join("does-not-exist");

        let error = CurrentDirGuard::change_to(&missing).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to set current directory")
        );
    }

    #[test]
    fn guards_serialize_across_threads() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        // Each thread holds a guard while incrementing a shared counter; if the
        // process-wide lock serializes guards, the counter never exceeds one.
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let live = Arc::clone(&live);
                let peak = Arc::clone(&peak);
                std::thread::spawn(move || {
                    let _guard = CurrentDirGuard::pin().unwrap();
                    let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    live.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "guards must never be held concurrently across threads"
        );
    }

    #[test]
    fn nested_guards_on_one_thread_do_not_deadlock() {
        // The reentrant lock allows a thread to hold nested guards; a
        // non-reentrant lock would deadlock here instead.
        let _outer = CurrentDirGuard::pin().unwrap();
        let _inner = CurrentDirGuard::pin().unwrap();
    }
}

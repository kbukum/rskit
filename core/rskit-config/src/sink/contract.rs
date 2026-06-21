use std::fmt;

use rskit_errors::AppResult;
use rskit_util::SecretString;

/// Adapter contract for writable configuration backends.
///
/// A `ConfigSink` persists or patches configuration values back to a backend —
/// a file, an in-memory store, or a future remote backend such as Vault, SSM, or
/// a Kubernetes secret. `rskit-config` owns the contract; concrete backends live
/// in their own adapter crates and depend only on this trait.
///
/// Sinks are injected explicitly (never via a global registry) and are
/// object-safe, so callers can hold a `Box<dyn ConfigSink>`.
///
/// Values flow as [`SecretString`] end-to-end. Implementations must never log
/// the plaintext value; only the key may appear in diagnostics. Writing the
/// plaintext to the backing store (e.g. a file) is the sink's intended, explicit
/// persistence — not a leak.
pub trait ConfigSink: fmt::Debug + Send + Sync + 'static {
    /// Set or replace the value stored at `key`.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`](rskit_errors::AppError) (cause preserved) if
    /// the backend rejects the write or is unreachable.
    fn set(&self, key: &str, value: SecretString) -> AppResult<()>;

    /// Remove the value stored at `key`.
    ///
    /// Removing a missing key succeeds (idempotent), unless the backend
    /// distinguishes absence as an error.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`](rskit_errors::AppError) (cause preserved) if
    /// the backend rejects the removal or is unreachable.
    fn remove(&self, key: &str) -> AppResult<()>;

    /// Set many key/value pairs.
    ///
    /// The default applies each entry with [`set`](ConfigSink::set) in order and
    /// fails fast on the first error, leaving earlier writes applied. Adapters
    /// with native batch or transactional writes should override this for
    /// atomicity and efficiency.
    ///
    /// # Errors
    ///
    /// Returns the first per-key error encountered.
    fn set_many(&self, entries: Vec<(String, SecretString)>) -> AppResult<()> {
        for (key, value) in entries {
            self.set(&key, value)?;
        }
        Ok(())
    }
}

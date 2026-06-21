use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_errors::AppResult;
use rskit_util::SecretString;

use super::ConfigSink;

/// Shared mutable state behind an [`InMemoryConfigSink`].
#[derive(Debug, Default)]
struct State {
    values: BTreeMap<String, SecretString>,
}

/// In-memory writable config store.
///
/// A process-local reference [`ConfigSink`] backed by a `BTreeMap`. Useful for
/// tests, defaults, and composing override layers without touching disk or a
/// remote backend. Cheaply cloneable; clones share the same underlying store.
///
/// With the `watch` feature enabled it also implements
/// [`ConfigWatch`](crate::ConfigWatch): every [`set`](ConfigSink::set) and
/// [`remove`](ConfigSink::remove) fans a change event out to active subscribers.
#[derive(Debug, Clone, Default)]
pub struct InMemoryConfigSink {
    state: Arc<Mutex<State>>,
    #[cfg(feature = "watch")]
    broadcaster: rskit_pipeline::Broadcaster<crate::watch::ConfigChange>,
}

impl InMemoryConfigSink {
    /// Create an empty in-memory sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the masked value stored at `key`, if present.
    ///
    /// The returned [`SecretString`] masks its plaintext in display, debug, and
    /// serialization; use [`SecretString::expose`] to read the plaintext
    /// intentionally.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<SecretString> {
        self.state.lock().values.get(key).cloned()
    }

    /// Return the number of stored keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state.lock().values.len()
    }

    /// Return `true` if the store holds no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.lock().values.is_empty()
    }
}

impl ConfigSink for InMemoryConfigSink {
    fn set(&self, key: &str, value: SecretString) -> AppResult<()> {
        self.state.lock().values.insert(key.to_string(), value);
        #[cfg(feature = "watch")]
        self.broadcaster
            .broadcast(&crate::watch::ConfigChange::Set {
                key: key.to_string(),
            });
        Ok(())
    }

    fn remove(&self, key: &str) -> AppResult<()> {
        self.state.lock().values.remove(key);
        #[cfg(feature = "watch")]
        self.broadcaster
            .broadcast(&crate::watch::ConfigChange::Removed {
                key: key.to_string(),
            });
        Ok(())
    }
}

#[cfg(feature = "watch")]
impl crate::watch::ConfigWatch for InMemoryConfigSink {
    fn watch(
        &self,
        cancel: tokio_util::sync::CancellationToken,
    ) -> AppResult<crate::watch::ConfigChangeStream> {
        Ok(self.broadcaster.subscribe(cancel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_round_trips_plaintext() {
        let sink = InMemoryConfigSink::new();
        sink.set("api_token", SecretString::new("s3cret")).unwrap();
        assert_eq!(sink.get("api_token").unwrap().expose(), "s3cret");
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn set_replaces_existing_value() {
        let sink = InMemoryConfigSink::new();
        sink.set("k", SecretString::new("old")).unwrap();
        sink.set("k", SecretString::new("new")).unwrap();
        assert_eq!(sink.get("k").unwrap().expose(), "new");
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn remove_deletes_key_and_is_idempotent() {
        let sink = InMemoryConfigSink::new();
        sink.set("k", SecretString::new("v")).unwrap();
        sink.remove("k").unwrap();
        assert!(sink.get("k").is_none());
        sink.remove("k").unwrap();
        assert!(sink.is_empty());
    }

    #[test]
    fn set_many_applies_all_entries() {
        let sink = InMemoryConfigSink::new();
        sink.set_many(vec![
            ("a".to_string(), SecretString::new("1")),
            ("b".to_string(), SecretString::new("2")),
        ])
        .unwrap();
        assert_eq!(sink.get("a").unwrap().expose(), "1");
        assert_eq!(sink.get("b").unwrap().expose(), "2");
    }

    #[test]
    fn debug_does_not_leak_secret() {
        let sink = InMemoryConfigSink::new();
        sink.set("k", SecretString::new("topsecret")).unwrap();
        let rendered = format!("{sink:?}");
        assert!(!rendered.contains("topsecret"));
    }

    #[test]
    fn clones_share_state() {
        let sink = InMemoryConfigSink::new();
        let clone = sink.clone();
        sink.set("k", SecretString::new("v")).unwrap();
        assert_eq!(clone.get("k").unwrap().expose(), "v");
    }
}

//! In-memory store implementation.

use std::collections::VecDeque;

use parking_lot::Mutex;
use tokio::time::Instant;

use rskit_errors::AppResult;

use crate::store::Store;

struct State<V> {
    values: VecDeque<V>,
    last_activity: Instant,
}

/// In-memory [`Store`] backed by a FIFO queue.
pub struct MemoryStore<V>
where
    V: Clone,
{
    state: Mutex<State<V>>,
}

impl<V> MemoryStore<V>
where
    V: Clone,
{
    /// Create an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                values: VecDeque::new(),
                last_activity: Instant::now(),
            }),
        }
    }
}

impl<V> Default for MemoryStore<V>
where
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Store<V> for MemoryStore<V>
where
    V: Clone + Send + Sync + 'static,
{
    fn append(&self, value: V) -> AppResult<()> {
        let mut state = self.state.lock();
        state.values.push_back(value);
        state.last_activity = Instant::now();
        Ok(())
    }

    fn pop_oldest(&self) -> AppResult<Option<V>> {
        let mut state = self.state.lock();
        let value = state.values.pop_front();
        if value.is_some() {
            state.last_activity = Instant::now();
        }
        Ok(value)
    }

    fn snapshot(&self) -> AppResult<Vec<V>> {
        let state = self.state.lock();
        Ok(state.values.iter().cloned().collect())
    }

    fn flush(&self) -> AppResult<Vec<V>> {
        let mut state = self.state.lock();
        let values = state.values.drain(..).collect();
        state.last_activity = Instant::now();
        Ok(values)
    }

    fn size(&self) -> AppResult<usize> {
        Ok(self.state.lock().values.len())
    }

    fn touch(&self) -> AppResult<()> {
        self.state.lock().last_activity = Instant::now();
        Ok(())
    }

    fn last_activity(&self) -> AppResult<Instant> {
        Ok(self.state.lock().last_activity)
    }

    fn close(&self) -> AppResult<()> {
        self.state.lock().values.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_supports_fifo_operations() {
        let store = MemoryStore::new();
        store.append(1).unwrap();
        store.append(2).unwrap();
        assert_eq!(store.snapshot().unwrap(), vec![1, 2]);
        assert_eq!(store.pop_oldest().unwrap(), Some(1));
        assert_eq!(store.flush().unwrap(), vec![2]);
    }

    #[test]
    fn default_store_tracks_activity_size_and_close() {
        let store = MemoryStore::default();
        let before = store.last_activity().unwrap();
        store.append(1).unwrap();
        assert_eq!(store.size().unwrap(), 1);
        assert!(store.last_activity().unwrap() >= before);
        assert_eq!(store.pop_oldest().unwrap(), Some(1));
        assert_eq!(store.pop_oldest().unwrap(), None);
        store.touch().unwrap();
        store.append(2).unwrap();
        store.close().unwrap();
        assert_eq!(store.size().unwrap(), 0);
    }
}

use std::collections::VecDeque;

use parking_lot::Mutex;

use rskit_errors::{AppError, AppResult};

/// A generic mock that records calls and returns pre-configured responses.
///
/// Useful for testing code that depends on a `RequestResponse`-style provider
/// without reaching real infrastructure.
///
/// # Example
///
/// ```ignore
/// let mock = MockProvider::<String, u64>::new();
/// mock.will_return(42);
///
/// // Hand `mock` to the system under test, then verify:
/// assert_eq!(mock.call_count(), 1);
/// ```
pub struct MockProvider<I, O> {
    responses: Mutex<VecDeque<AppResult<O>>>,
    calls: Mutex<Vec<I>>,
}

impl<I: Clone + Send + Sync + 'static, O: Clone + Send + Sync + 'static> MockProvider<I, O> {
    /// Create a new mock with no pre-configured responses.
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Enqueue a successful response.
    pub fn will_return(&self, response: O) -> &Self {
        self.responses.lock().push_back(Ok(response));
        self
    }

    /// Enqueue an error response.
    pub fn will_fail(&self, err: AppError) -> &Self {
        self.responses.lock().push_back(Err(err));
        self
    }

    /// Return a clone of all recorded call inputs.
    pub fn calls(&self) -> Vec<I> {
        self.calls.lock().clone()
    }

    /// Return the number of calls recorded so far.
    pub fn call_count(&self) -> usize {
        self.calls.lock().len()
    }

    /// Record a call and pop the next response.
    ///
    /// Panics if no response has been enqueued.
    pub fn execute(&self, input: I) -> AppResult<O> {
        self.calls.lock().push(input);
        self.responses
            .lock()
            .pop_front()
            .expect("MockProvider: no more responses enqueued")
    }
}

impl<I: Clone + Send + Sync + 'static, O: Clone + Send + Sync + 'static> Default
    for MockProvider<I, O>
{
    fn default() -> Self {
        Self::new()
    }
}

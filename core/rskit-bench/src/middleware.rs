//! Evaluator middleware: timing and caching wrappers.

use crate::evaluator::Evaluator;
use crate::types::Prediction;
use rskit_errors::AppResult;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Wraps an evaluator to record per-sample execution timings.
pub struct TimingMiddleware<L> {
    inner: Box<dyn Evaluator<L>>,
    timings: Arc<Mutex<Vec<(String, Duration)>>>,
}

impl<L: Send + Sync + Clone + 'static> TimingMiddleware<L> {
    pub fn new(inner: Box<dyn Evaluator<L>>) -> Self {
        Self {
            inner,
            timings: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn timings(&self) -> Vec<(String, Duration)> {
        self.timings.lock().clone()
    }

    pub fn average(&self) -> Duration {
        let t = self.timings.lock();
        if t.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = t.iter().map(|(_, d)| *d).sum();
        total / t.len() as u32
    }
}

#[async_trait::async_trait]
impl<L: Send + Sync + Clone + 'static> Evaluator<L> for TimingMiddleware<L> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }


    async fn evaluate(&self, input: Vec<u8>) -> AppResult<Prediction<L>> {
        let start = Instant::now();
        let result = self.inner.evaluate(input).await;
        let elapsed = start.elapsed();
        if let Ok(ref pred) = result {
            self.timings.lock().push((pred.sample_id.clone(), elapsed));
        }
        result
    }
}

fn hash_bytes(data: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Wraps an evaluator with input-hash-keyed caching.
pub struct CachingMiddleware<L> {
    inner: Box<dyn Evaluator<L>>,
    cache: Arc<Mutex<HashMap<u64, Prediction<L>>>>,
    hits: Arc<Mutex<u64>>,
    misses: Arc<Mutex<u64>>,
}

impl<L: Send + Sync + Clone + 'static> CachingMiddleware<L> {
    pub fn new(inner: Box<dyn Evaluator<L>>) -> Self {
        Self {
            inner,
            cache: Arc::new(Mutex::new(HashMap::new())),
            hits: Arc::new(Mutex::new(0)),
            misses: Arc::new(Mutex::new(0)),
        }
    }

    pub fn hit_count(&self) -> u64 {
        *self.hits.lock()
    }

    pub fn miss_count(&self) -> u64 {
        *self.misses.lock()
    }
}

#[async_trait::async_trait]
impl<L: Send + Sync + Clone + 'static> Evaluator<L> for CachingMiddleware<L> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }


    async fn evaluate(&self, input: Vec<u8>) -> AppResult<Prediction<L>> {
        let key = hash_bytes(&input);
        if let Some(cached) = self.cache.lock().get(&key).cloned() {
            *self.hits.lock() += 1;
            return Ok(cached);
        }
        *self.misses.lock() += 1;
        let result = self.inner.evaluate(input).await?;
        self.cache.lock().insert(key, result.clone());
        Ok(result)
    }
}

/// Convenience: wrap an evaluator with timing.
pub fn with_timing<L: Send + Sync + Clone + 'static>(
    eval: Box<dyn Evaluator<L>>,
) -> TimingMiddleware<L> {
    TimingMiddleware::new(eval)
}

/// Convenience: wrap an evaluator with caching.
pub fn with_caching<L: Send + Sync + Clone + 'static>(
    eval: Box<dyn Evaluator<L>>,
) -> CachingMiddleware<L> {
    CachingMiddleware::new(eval)
}

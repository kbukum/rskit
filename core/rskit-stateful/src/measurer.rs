//! Value measurers for stateful accumulators.

/// Measures the size of accumulated values.
pub trait Measurer<V>: Send + Sync {
    /// Return the measured size for `values`.
    fn measure(&self, values: &[V]) -> usize;
}

/// Counts accumulated items.
#[derive(Debug, Clone, Copy, Default)]
pub struct CountMeasurer;

impl<V> Measurer<V> for CountMeasurer {
    fn measure(&self, values: &[V]) -> usize {
        values.len()
    }
}

/// Measures accumulated byte-like values by total byte length.
#[derive(Debug, Clone, Copy, Default)]
pub struct ByteSizeMeasurer;

impl<V> Measurer<V> for ByteSizeMeasurer
where
    V: AsRef<[u8]>,
{
    fn measure(&self, values: &[V]) -> usize {
        values.iter().map(|value| value.as_ref().len()).sum()
    }
}

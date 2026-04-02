//! Embedding data types, distance metrics, and aggregation functions.

use serde::{Deserialize, Serialize};

/// An embedding vector with optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// The original text that was embedded, if available.
    pub text: Option<String>,
    /// The model used to generate this embedding.
    pub model: Option<String>,
}

impl Embedding {
    /// Create a new embedding from a vector.
    pub fn new(vector: Vec<f32>) -> Self {
        Self {
            vector,
            text: None,
            model: None,
        }
    }

    /// Set the source text for this embedding.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Set the model name for this embedding.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Return the dimensionality of this embedding.
    pub fn dimensions(&self) -> usize {
        self.vector.len()
    }
}

/// Compute the cosine similarity between two vectors.
///
/// Returns a value in `[-1.0, 1.0]` where `1.0` means identical direction.
/// Returns `0.0` if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have equal dimensions");

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Compute the Euclidean (L2) distance between two vectors.
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have equal dimensions");

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

/// Compute the dot product of two vectors.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have equal dimensions");

    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Compute the element-wise mean of a collection of vectors (mean pooling).
///
/// Returns `None` if the input is empty.
pub fn mean_pooling(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    if vectors.is_empty() {
        return None;
    }

    let dims = vectors[0].len();
    let count = vectors.len() as f32;
    let mut result = vec![0.0f32; dims];

    for v in vectors {
        assert_eq!(v.len(), dims, "all vectors must have equal dimensions");
        for (i, val) in v.iter().enumerate() {
            result[i] += val;
        }
    }

    for val in &mut result {
        *val /= count;
    }

    Some(result)
}

/// Compute the element-wise maximum of a collection of vectors (max pooling).
///
/// Returns `None` if the input is empty.
pub fn max_pooling(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    if vectors.is_empty() {
        return None;
    }

    let dims = vectors[0].len();
    let mut result = vec![f32::NEG_INFINITY; dims];

    for v in vectors {
        assert_eq!(v.len(), dims, "all vectors must have equal dimensions");
        for (i, val) in v.iter().enumerate() {
            if *val > result[i] {
                result[i] = *val;
            }
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![1.0, 2.0];
        let zero = vec![0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &zero), 0.0);
    }

    #[test]
    fn euclidean_distance_same_point() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((euclidean_distance(&v, &v)).abs() < 1e-6);
    }

    #[test]
    fn euclidean_distance_known_value() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn dot_product_known_value() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((dot_product(&a, &b) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn mean_pooling_empty() {
        let empty: Vec<Vec<f32>> = vec![];
        assert!(mean_pooling(&empty).is_none());
    }

    #[test]
    fn mean_pooling_single() {
        let vectors = vec![vec![2.0, 4.0]];
        let result = mean_pooling(&vectors).unwrap();
        assert!((result[0] - 2.0).abs() < 1e-6);
        assert!((result[1] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn mean_pooling_multiple() {
        let vectors = vec![vec![1.0, 3.0], vec![3.0, 1.0]];
        let result = mean_pooling(&vectors).unwrap();
        assert!((result[0] - 2.0).abs() < 1e-6);
        assert!((result[1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn max_pooling_empty() {
        let empty: Vec<Vec<f32>> = vec![];
        assert!(max_pooling(&empty).is_none());
    }

    #[test]
    fn max_pooling_selects_max() {
        let vectors = vec![vec![1.0, 4.0], vec![3.0, 2.0]];
        let result = max_pooling(&vectors).unwrap();
        assert!((result[0] - 3.0).abs() < 1e-6);
        assert!((result[1] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn embedding_new_and_builders() {
        let e = Embedding::new(vec![1.0, 2.0])
            .with_text("hello")
            .with_model("test-model");
        assert_eq!(e.dimensions(), 2);
        assert_eq!(e.text.unwrap(), "hello");
        assert_eq!(e.model.unwrap(), "test-model");
    }
}

//! Vector math helpers shared across AI crates.

/// Compute the cosine similarity between two vectors.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Compute the Euclidean (L2) distance between two vectors.
#[must_use]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::NAN;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

/// Compute the dot product of two vectors.
#[must_use]
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Compute the element-wise mean of a collection of vectors.
#[must_use]
pub fn mean_pooling(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    let first = vectors.first()?;
    let dims = first.len();
    if vectors.iter().any(|vector| vector.len() != dims) {
        return None;
    }
    let count = vectors.len() as f32;
    let mut result = vec![0.0_f32; dims];
    for vector in vectors {
        for (index, value) in vector.iter().enumerate() {
            result[index] += value;
        }
    }
    for value in &mut result {
        *value /= count;
    }
    Some(result)
}

/// Compute the element-wise maximum of a collection of vectors.
#[must_use]
pub fn max_pooling(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    let first = vectors.first()?;
    let dims = first.len();
    if vectors.iter().any(|vector| vector.len() != dims) {
        return None;
    }
    let mut result = vec![f32::NEG_INFINITY; dims];
    for vector in vectors {
        for (index, value) in vector.iter().enumerate() {
            if *value > result[index] {
                result[index] = *value;
            }
        }
    }
    Some(result)
}

/// Normalize a vector to unit length.
#[must_use]
pub fn normalize(vector: &[f32]) -> Option<Vec<f32>> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        return None;
    }
    Some(vector.iter().map(|value| value / norm).collect())
}

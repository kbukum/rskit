/// Split a slice of items into chunks of a given maximum size.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::chunk;
/// let items = vec![1, 2, 3, 4, 5];
/// let chunks = chunk(&items, 2);
/// assert_eq!(chunks, vec![vec![&1, &2], vec![&3, &4], vec![&5]]);
/// ```
pub fn chunk<T>(items: &[T], size: usize) -> Vec<Vec<&T>> {
    if size == 0 {
        return Vec::new();
    }
    items.chunks(size).map(|c| c.iter().collect()).collect()
}

/// Split a vector of items into owned chunks of a given maximum size, consuming the vector.
pub fn chunk_owned<T>(items: Vec<T>, size: usize) -> Vec<Vec<T>> {
    if size == 0 || items.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut iter = items.into_iter();
    loop {
        let chunk: Vec<T> = iter.by_ref().take(size).collect();
        if chunk.is_empty() {
            break;
        }
        chunks.push(chunk);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(chunk(&items, 2), vec![vec![&1, &2], vec![&3, &4], vec![&5]]);
        assert_eq!(chunk(&items, 0), Vec::<Vec<&i32>>::new());
    }

    #[test]
    fn test_chunk_owned() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(chunk_owned(items, 2), vec![vec![1, 2], vec![3, 4], vec![5]]);
    }
}

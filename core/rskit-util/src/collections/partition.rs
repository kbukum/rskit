/// Split a collection into two collections based on a predicate function.
///
/// The first vector contains elements where the predicate is true,
/// and the second vector contains elements where the predicate is false.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::partition;
/// let items = vec![1, 2, 3, 4, 5];
/// let (evens, odds) = partition(items, |n| n % 2 == 0);
/// assert_eq!(evens, vec![2, 4]);
/// assert_eq!(odds, vec![1, 3, 5]);
/// ```
pub fn partition<T, F>(items: impl IntoIterator<Item = T>, mut predicate: F) -> (Vec<T>, Vec<T>)
where
    F: FnMut(&T) -> bool,
{
    let mut truthy = Vec::new();
    let mut falsy = Vec::new();
    for item in items {
        if predicate(&item) {
            truthy.push(item);
        } else {
            falsy.push(item);
        }
    }
    (truthy, falsy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition() {
        let items = vec![1, 2, 3, 4, 5];
        let (evens, odds) = partition(items, |n| n % 2 == 0);
        assert_eq!(evens, vec![2, 4]);
        assert_eq!(odds, vec![1, 3, 5]);
    }
}

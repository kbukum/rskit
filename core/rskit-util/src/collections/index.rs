use std::collections::HashMap;
use std::hash::Hash;

/// Convert a collection into a `HashMap` mapping a chosen key to the unique item.
/// If there are duplicate keys, the last one wins.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::index_by;
/// let items = vec!["apple", "banana", "cherry"];
/// let indexed = index_by(items, |s| s.len());
/// assert_eq!(indexed.get(&5).unwrap(), &"apple");
/// assert_eq!(indexed.get(&6).unwrap(), &"cherry");
/// ```
pub fn index_by<T, K, F>(items: impl IntoIterator<Item = T>, mut key_selector: F) -> HashMap<K, T>
where
    K: Eq + Hash,
    F: FnMut(&T) -> K,
{
    let mut map = HashMap::new();
    for item in items {
        let key = key_selector(&item);
        map.insert(key, item);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_by() {
        let items = vec!["apple", "banana", "cherry"];
        let indexed = index_by(items, |s| s.len());
        assert_eq!(indexed.get(&5).unwrap(), &"apple");
        assert_eq!(indexed.get(&6).unwrap(), &"cherry");
    }
}

use std::collections::HashMap;
use std::hash::Hash;

/// Group elements of an iterator/vec into a `HashMap` by a key selector function.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::group_by;
/// let items = vec!["apple", "banana", "apricot", "cherry"];
/// let grouped = group_by(items, |s| s.chars().next().unwrap());
/// assert_eq!(grouped.get(&'a').unwrap(), &vec!["apple", "apricot"]);
/// ```
pub fn group_by<T, K, F>(
    items: impl IntoIterator<Item = T>,
    mut key_selector: F,
) -> HashMap<K, Vec<T>>
where
    K: Eq + Hash,
    F: FnMut(&T) -> K,
{
    let mut map = HashMap::new();
    for item in items {
        let key = key_selector(&item);
        map.entry(key).or_insert_with(Vec::new).push(item);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_by() {
        let items = vec!["apple", "banana", "apricot", "cherry"];
        let grouped = group_by(items, |s| s.chars().next().unwrap());
        assert_eq!(grouped.get(&'a').unwrap(), &vec!["apple", "apricot"]);
        assert_eq!(grouped.get(&'b').unwrap(), &vec!["banana"]);
        assert_eq!(grouped.get(&'c').unwrap(), &vec!["cherry"]);
    }
}

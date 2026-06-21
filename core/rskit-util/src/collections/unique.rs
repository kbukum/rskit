use std::collections::HashSet;
use std::hash::Hash;

/// Return the keys that occur more than once in `items`, in first-duplicate
/// order.
///
/// `key_selector` extracts the identity key from each item. Each key that
/// appears two or more times is reported exactly once, in the order its first
/// duplicate occurrence is encountered. Items with unique keys are not reported.
///
/// This is the low-level primitive; see [`ensure_unique_by`] for a convenience
/// that turns any duplicate into an error. The helper is error-policy-neutral —
/// it returns the offending keys so the caller can wrap them in whatever domain
/// error carries the right context.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::find_duplicates_by;
///
/// let items = vec!["apple", "avocado", "banana", "cherry", "cantaloupe"];
/// let dups = find_duplicates_by(&items, |s| s.chars().next().unwrap());
/// assert_eq!(dups, vec!['a', 'c']);
/// ```
pub fn find_duplicates_by<T, K, F>(items: impl IntoIterator<Item = T>, mut key_selector: F) -> Vec<K>
where
    K: Eq + Hash + Clone,
    F: FnMut(&T) -> K,
{
    let mut seen = HashSet::new();
    let mut reported = HashSet::new();
    let mut duplicates = Vec::new();
    for item in items {
        let key = key_selector(&item);
        if !seen.insert(key.clone()) && reported.insert(key.clone()) {
            duplicates.push(key);
        }
    }
    duplicates
}

/// Ensure every item in `items` has a unique key, returning the first duplicate.
///
/// `key_selector` extracts the identity key from each item. Returns `Ok(())`
/// when all keys are unique, or `Err(key)` with the first key seen twice. The
/// caller maps the returned key to a domain error.
///
/// # Errors
///
/// Returns the first duplicate key as `Err(K)`.
///
/// # Examples
///
/// ```
/// use rskit_util::collections::ensure_unique_by;
///
/// assert!(ensure_unique_by(&["a", "b", "c"], |s| *s).is_ok());
/// let dup: &str = ensure_unique_by(&["a", "b", "a"], |s| *s).unwrap_err();
/// assert_eq!(dup, "a");
/// ```
pub fn ensure_unique_by<T, K, F>(
    items: impl IntoIterator<Item = T>,
    mut key_selector: F,
) -> Result<(), K>
where
    K: Eq + Hash,
    F: FnMut(&T) -> K,
{
    let mut seen = HashSet::new();
    for item in items {
        let key = key_selector(&item);
        if !seen.insert(key) {
            // Re-extract to return the owned key without requiring `Clone`.
            return Err(key_selector(&item));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_unique_by, find_duplicates_by};

    #[test]
    fn find_duplicates_reports_each_key_once_in_order() {
        let items = vec!["apple", "avocado", "banana", "cherry", "cantaloupe", "apricot"];
        let dups = find_duplicates_by(&items, |s| s.chars().next().unwrap());
        assert_eq!(dups, vec!['a', 'c']);
    }

    #[test]
    fn find_duplicates_empty_when_all_unique() {
        let items = vec!["a", "b", "c"];
        let dups = find_duplicates_by(&items, |s| *s);
        assert!(dups.is_empty());
    }

    #[test]
    fn ensure_unique_ok_when_all_unique() {
        assert!(ensure_unique_by(&["a", "b", "c"], |s| *s).is_ok());
    }

    #[test]
    fn ensure_unique_returns_first_duplicate() {
        let err: &str = ensure_unique_by(&["a", "b", "a", "b"], |s| *s).unwrap_err();
        assert_eq!(err, "a");
    }
}

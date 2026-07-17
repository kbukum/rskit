/// Join an optional storage prefix and object key using normalized `/` separators.
#[must_use]
pub fn prefixed_key(prefix: Option<&str>, key: &str) -> String {
    let key = key.trim_start_matches('/');
    match prefix
        .map(str::trim)
        .map(|prefix| prefix.trim_matches('/'))
        .filter(|prefix| !prefix.is_empty())
    {
        Some(prefix) if key.is_empty() => format!("{prefix}/"),
        Some(prefix) => format!("{prefix}/{key}"),
        None => key.to_string(),
    }
}

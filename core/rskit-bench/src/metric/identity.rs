//! Delimiter-safe identity components for metric names and provenance.

/// Escapes every character used to join or delimit identity components — `\`, `/`, `@`, `[`, `]`, `#`, and `:` — so a component can never be confused with the delimiters that frame and join components.
///
/// Metric names frame identity in `NAME[...]` and join components with `/`, `@`, `#`, and `:` (e.g. `llm_judge[provider/model@id@version#fingerprint:t0.5]`); escaping each component first keeps the framing and joins unambiguous, so `Custom("a/b")` + name `c` and `Custom("a")` + name `b/c` yield distinct identities rather than a colliding `a/b/c`, and a component containing `#`, `:`, or `]` cannot alias across the fingerprint, threshold, or closing delimiter.
pub(crate) fn escape_component(component: &str) -> String {
    component
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('@', "\\@")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('#', "\\#")
        .replace(':', "\\:")
}

#[cfg(test)]
mod tests {
    use super::escape_component;

    #[test]
    fn escapes_all_identity_delimiters() {
        assert_eq!(escape_component("a\\b"), "a\\\\b");
        assert_eq!(escape_component("a/b"), "a\\/b");
        assert_eq!(escape_component("a@b"), "a\\@b");
        assert_eq!(escape_component("a[b"), "a\\[b");
        assert_eq!(escape_component("a]b"), "a\\]b");
        assert_eq!(escape_component("a#b"), "a\\#b");
        assert_eq!(escape_component("a:b"), "a\\:b");
    }

    #[test]
    fn framing_delimiters_are_backslash_escaped() {
        // `[`, `]`, `#`, and `:` frame `NAME[...]`, close it, join the fingerprint, and prefix the threshold; escaping each keeps a component that contains one from aliasing across the frame, fingerprint, or threshold boundary.
        assert_eq!(escape_component("model]:t0.5#fp"), "model\\]\\:t0.5\\#fp");
    }
}

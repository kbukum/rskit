//! Delimiter-safe identity components for metric names and provenance.

/// Escapes `\`, `/`, and `@` so an identity component can never be confused with the delimiters that join components.
///
/// Metrics embed model/prompt identities into result names joined by `/` and `@`; escaping each component first keeps the join unambiguous, so `Custom("a/b")` + name `c` and `Custom("a")` + name `b/c` yield distinct identities rather than a colliding `a/b/c`.
pub(crate) fn escape_component(component: &str) -> String {
    component
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('@', "\\@")
}

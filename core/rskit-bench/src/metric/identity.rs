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

/// A floating-point threshold that can be rendered into a metric identity.
///
/// Implemented for `f64` and `f32` so [`format_threshold`] can canonicalize either width without converting between them — an `f32`→`f64` widening would turn a clean `0.8` cutoff into `0.800000011920929`, splitting one identity in two.
pub(crate) trait Threshold: Copy + std::fmt::Display + PartialEq {
    /// The additive identity for this width, used to fold signed zero.
    fn zero() -> Self;
}

impl Threshold for f64 {
    fn zero() -> Self {
        0.0
    }
}

impl Threshold for f32 {
    fn zero() -> Self {
        0.0
    }
}

/// Renders a range-validated threshold into its single canonical identity string.
///
/// A cutoff has exactly one textual identity: negative zero is folded to positive zero (`-0.0` and `0.0` are the same threshold, but `f64`'s `Display` renders them as `-0` and `0`, which would split one cutoff into two metric names and let [`RunComparator`](crate::compare::RunComparator) diff an incomparable delta). Every other value uses the default `Display`, which is already the shortest round-trippable decimal, so distinct cutoffs stay distinct. This mirrors gokit's `formatThreshold` so both kits render the same identity for the same cutoff.
pub(crate) fn format_threshold<T: Threshold>(threshold: T) -> String {
    // `-0.0 == 0.0` is true for floats, so this folds signed zero to `+0.0`.
    let canonical = if threshold == T::zero() {
        T::zero()
    } else {
        threshold
    };
    format!("{canonical}")
}

#[cfg(test)]
mod tests {
    use super::{escape_component, format_threshold};

    #[test]
    fn format_threshold_renders_shortest_round_trippable() {
        assert_eq!(format_threshold(0.5_f64), "0.5");
        assert_eq!(format_threshold(0.8_f64), "0.8");
        assert_eq!(format_threshold(1.0_f64), "1");
        assert_eq!(format_threshold(0.8_f32), "0.8");
    }

    #[test]
    fn format_threshold_folds_negative_zero() {
        assert_eq!(format_threshold(-0.0_f64), "0");
        assert_eq!(format_threshold(0.0_f64), "0");
        assert_eq!(format_threshold(-0.0_f32), "0");
    }

    #[test]
    fn format_threshold_keeps_distinct_cutoffs_distinct() {
        assert_ne!(format_threshold(0.50001_f64), format_threshold(0.50002_f64));
    }

    #[test]
    fn negative_zero_and_zero_yield_identical_names() {
        let zero = format!("classification[t{}]", format_threshold(0.0_f64));
        let neg_zero = format!("classification[t{}]", format_threshold(-0.0_f64));
        assert_eq!(zero, neg_zero);
    }

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

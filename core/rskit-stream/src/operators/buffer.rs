//! Shared accumulation buffer helpers for the windowing and rate operators.
//!
//! Both the leading-edge windowing operators ([`crate::operators::windowing`]) and the
//! trailing-edge rate operators ([`crate::operators::rate`]) accumulate items into a
//! `Vec<T>` bounded by a `max_items` cap and flush it as one window. These
//! helpers keep that buffer discipline in one place so the two families stay
//! consistent instead of each re-deriving the capacity math.

/// Upper bound on the initial `Vec` capacity reserved for an accumulation
/// window, so a very large `max_items` cap does not eagerly allocate a huge
/// buffer up front.
pub(super) const WINDOW_INITIAL_CAPACITY_LIMIT: usize = 1024;

/// Clamp `max_items` to at least one and derive the initial buffer capacity,
/// bounded by [`WINDOW_INITIAL_CAPACITY_LIMIT`].
///
/// Returns `(max_items, initial_capacity)`.
pub(super) fn window_capacity(max_items: usize) -> (usize, usize) {
    let max_items = max_items.max(1);
    (max_items, max_items.min(WINDOW_INITIAL_CAPACITY_LIMIT))
}

/// Take the accumulated window, leaving a fresh empty buffer sized for the next
/// window in its place.
pub(super) fn take_window<T>(buf: &mut Vec<T>, next_capacity: usize) -> Vec<T> {
    std::mem::replace(buf, Vec::with_capacity(next_capacity))
}

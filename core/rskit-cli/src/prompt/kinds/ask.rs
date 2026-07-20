//! The invariant presentation context threaded through every prompt kind.

use crate::prompt::mode::PromptMode;
use crate::prompt::render::Style;

/// The parts of a prompt that stay constant across a single interaction:
/// how it is rendered ([`Style`]), whether the session is interactive
/// ([`PromptMode`]), and the question text.
///
/// Grouping these into one `Copy` context keeps each kind's dispatch and draw
/// helpers readable and mis-order safe, and lets the terminal stay a separate
/// `&mut` argument so it can be re-borrowed through
/// [`with_raw_mode`](super::with_raw_mode).
#[derive(Clone, Copy)]
pub(crate) struct Ask<'a> {
    /// The palette and glyphs used to render the prompt.
    pub style: Style,
    /// Whether the session is interactive or resolves to declared defaults.
    pub mode: PromptMode,
    /// The question text shown to the user.
    pub prompt: &'a str,
}

//! Pseudoterminal-backed process I/O.
//!
//! Pipe-backed modes give deterministic capture, but a child connected to a pipe sees a non-terminal
//! and disables colors, progress bars, and other tty-gated rendering.
//! This module allocates a real pseudoterminal
//! so the child renders exactly as it would in an interactive shell, while the parent still observes
//! and (optionally) captures the merged output stream.
//!
//! PTY support is Unix-only. The public surface is [`PtyIo`], [`PtySize`], and [`terminal_size`];
//! allocation and child setup are crate-internal.

mod alloc;
mod child;
mod config;
mod master;
mod size;

pub(crate) use alloc::{PtyPair, open_pty};
pub(crate) use child::install_controlling_tty;
pub use config::PtyIo;
pub(crate) use master::PtyMaster;
pub use size::{PtySize, terminal_size};

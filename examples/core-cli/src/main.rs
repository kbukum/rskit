//! `core-cli` binary entry point.
//!
//! A command-line tool built entirely on rskit **core** crates — no transport
//! or server crate, and no rskit facade. See `docs/CONSUMER-CLASSES.md`.
//!
//! Usage:
//!
//! ```text
//! core-cli version
//! core-cli show <config.toml>
//! core-cli run <units>
//! ```
//!
//! All routing, argument parsing, and exit-code mapping live in
//! [`core_cli::cli`] so they can be tested with only core crates; this entry
//! point is a thin shim around it. The `main` glue is gated behind
//! `#[cfg(not(test))]` (as in `agent-demo`) so it is excluded from the
//! coverage build rather than counted as permanently-uncovered lines.

#[cfg(not(test))]
use std::process::ExitCode as ProcessExit;

#[cfg(not(test))]
use core_cli::cli;
#[cfg(not(test))]
use rskit_cli::ErrorRenderer;

#[cfg(not(test))]
#[tokio::main]
async fn main() -> ProcessExit {
    match cli::dispatch(std::env::args().skip(1).collect()).await {
        Ok(()) => ProcessExit::SUCCESS,
        Err(err) => {
            let (rendered, code) = ErrorRenderer::default().render(&err);
            eprintln!("{rendered}");
            cli::to_process_exit(code)
        }
    }
}

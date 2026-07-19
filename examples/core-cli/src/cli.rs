//! Argv dispatch and process-exit wiring for `core-cli`.
//!
//! Kept in the library (rather than `main.rs`) so the command routing, argument parsing,
//! and exit-code mapping can be exercised directly by the integration tests using only core crates.

use std::process::ExitCode as ProcessExit;

use rskit_cli::{CancellationToken, ExitCode};
use rskit_errors::{AppError, AppResult};

use crate::commands;

/// Route `args` (the process arguments after the binary name) to the matching subcommand,
/// writing human-readable output to stdout.
///
/// # Errors
///
/// Returns a typed [`AppError`] for an unknown command, a missing argument,
/// or any failure surfaced by a subcommand (config loading, logging setup, ...).
pub async fn dispatch(args: Vec<String>) -> AppResult<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("version") => {
            print!("{}", commands::version::render());
            Ok(())
        }
        Some("show") => {
            let path = args
                .next()
                .ok_or_else(|| AppError::invalid_input("show", "missing <config.toml> argument"))?;
            let (_guard, rendered) = commands::show::execute(&path)?;
            print!("{rendered}");
            Ok(())
        }
        Some("run") => {
            let units = parse_units(args.next())?;
            let token = install_signal_handler();
            print!("{}", commands::run::execute(units, &token).await);
            Ok(())
        }
        other => Err(unknown_command(other)),
    }
}

/// Parse the `run` subcommand's `<units>` argument as a non-negative integer.
fn parse_units(arg: Option<String>) -> AppResult<u32> {
    arg.ok_or_else(|| AppError::invalid_input("run", "missing <units> argument"))?
        .parse::<u32>()
        .map_err(|err| {
            AppError::invalid_input("units", "expected a non-negative integer").with_cause(err)
        })
}

/// Build the typed error returned for an unrecognised subcommand.
fn unknown_command(command: Option<&str>) -> AppError {
    AppError::invalid_input(
        "command",
        format!(
            "unknown command `{}`; expected version|show|run",
            command.unwrap_or("<none>")
        ),
    )
}

/// Spawn a task that cancels the returned token on Ctrl+C.
///
/// Demonstrates lifecycle ownership using only core crates:
/// the caller owns the returned [`CancellationToken`] and the work loop races it to wind down promptly.
///
/// The Ctrl+C watcher is only spawned when a Tokio runtime is available,
/// so calling this outside a runtime returns a usable token instead of panicking.
#[must_use]
pub fn install_signal_handler() -> CancellationToken {
    let token = CancellationToken::new();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let trigger = token.clone();
        handle.spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                trigger.cancel();
            }
        });
    }
    token
}

/// Map a [`rskit_cli::ExitCode`] to a process exit status.
#[must_use]
pub fn to_process_exit(code: ExitCode) -> ProcessExit {
    ProcessExit::from(exit_status(code))
}

/// Narrow an [`ExitCode`] to the `u8` accepted by [`ProcessExit`],
/// defaulting to a generic failure if the code does not fit.
#[must_use]
pub fn exit_status(code: ExitCode) -> u8 {
    u8::try_from(code.as_i32()).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_status_maps_known_codes() {
        assert_eq!(exit_status(ExitCode::Success), 0);
        assert_eq!(exit_status(ExitCode::Usage), 2);
        assert_eq!(exit_status(ExitCode::Failure), 1);
    }

    #[test]
    fn parse_units_accepts_integer() {
        assert_eq!(parse_units(Some("7".to_string())).expect("valid"), 7);
    }

    #[test]
    fn parse_units_rejects_missing() {
        let err = parse_units(None).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn parse_units_rejects_non_integer() {
        let err = parse_units(Some("nope".to_string())).unwrap_err();
        assert!(err.to_string().contains("integer"));
    }

    #[test]
    fn unknown_command_reports_command() {
        assert!(unknown_command(Some("bogus")).to_string().contains("bogus"));
        assert!(unknown_command(None).to_string().contains("<none>"));
    }

    #[test]
    fn install_signal_handler_does_not_panic_without_runtime() {
        // Called outside any Tokio runtime: must return a token, not panic.
        let token = install_signal_handler();
        assert!(!token.is_cancelled());
    }
}
